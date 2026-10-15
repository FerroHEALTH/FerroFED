// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The PostgreSQL store of the configuration revisions against a real
//! PostgreSQL 18: two gateway replicas on one database share the history
//! and the running mark, a save on a stale base and a move of the mark from
//! where another replica left it are refused with nothing written, replicas
//! racing on one base store exactly one revision, a reopened store holds
//! everything, and a revision one gateway applies reaches a second gateway
//! on the same database through its watch of the running mark. No
//! specification governs this: our own design.

use std::sync::{Arc, Barrier};
use std::time::Duration;

use ferrofed_registry::secret::SecretUrl;
use ferrofed_server::revision::postgres::PostgresRevisions;
use ferrofed_server::revision::watch;
use ferrofed_server::revision::{
    Author, Current, Draft, Origin, Outcome, RevisionId, RevisionStore, Saved, Swapped,
};
use ferrofed_testkit::containers::{self, database};
use http::StatusCode;
use jiff::Timestamp;

use crate::e2e::TestResult;
use crate::revisions::{Gateway, Standing, member, rest};

/// How many replicas race on one base.
const RACERS: usize = 6;

/// A draft by the file with the configuration `toml`.
fn draft(toml: &str) -> Draft {
    Draft {
        origin: Origin::File,
        restores: None,
        author: Author::File,
        at: Timestamp::UNIX_EPOCH,
        reason: String::from("synthetic"),
        toml: toml.to_owned(),
    }
}

/// The mark of `revision` over `previous`.
fn mark(revision: RevisionId, previous: Option<RevisionId>) -> Current {
    Current {
        revision,
        previous,
        outcome: Outcome::Applied,
        needs_restart: Vec::new(),
        applied_by: Author::File,
        applied_at: Timestamp::UNIX_EPOCH,
    }
}

/// The revision numbered `number`.
fn id(number: u64) -> Result<RevisionId, &'static str> {
    RevisionId::new(number).ok_or("a revision number")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_replicas_share_the_history_and_the_mark() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres("config_revisions_shared", &[]).await?;
    let url = SecretUrl::new(server.url("config_revisions_shared"));
    tokio::task::block_in_place(|| -> TestResult {
        let one = PostgresRevisions::open(&url)?;
        let two = PostgresRevisions::open(&url)?;
        let Saved::Stored(first) = one.save(None, draft("a = 1\n"))? else {
            return Err("the first save is stored".into());
        };
        assert_eq!(RevisionId::FIRST, first.id);
        assert_eq!(
            Saved::Stale {
                head: Some(RevisionId::FIRST)
            },
            two.save(None, draft("a = 2\n"))?
        );
        let Saved::Stored(second) = two.save(Some(RevisionId::FIRST), draft("a = 2\n"))? else {
            return Err("a save on the newest is stored".into());
        };
        assert_eq!(
            Saved::Stale { head: Some(id(2)?) },
            one.save(Some(RevisionId::FIRST), draft("a = 3\n"))?
        );
        assert_eq!(vec![first.clone(), second.clone()], one.history()?);
        assert_eq!(one.history()?, two.history()?);
        assert_eq!(Some(second), one.revision(id(2)?)?);
        assert_eq!(None, two.revision(id(3)?)?);

        assert_eq!(
            Swapped::Swapped,
            one.swap_current(None, &mark(RevisionId::FIRST, None))?
        );
        assert_eq!(
            Swapped::Stale {
                current: Some(RevisionId::FIRST)
            },
            two.swap_current(None, &mark(id(2)?, None))?
        );
        let next = mark(id(2)?, Some(RevisionId::FIRST));
        assert_eq!(
            Swapped::Swapped,
            two.swap_current(Some(RevisionId::FIRST), &next)?
        );
        assert_eq!(Some(next.clone()), one.current()?);
        drop((one, two));
        let reopened = PostgresRevisions::open(&url)?;
        assert_eq!(Some(id(2)?), reopened.head()?);
        assert_eq!(Some(next), reopened.current()?);
        Ok(())
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn replicas_racing_on_one_base_store_exactly_one_revision() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres("config_revisions_race", &[]).await?;
    let url = SecretUrl::new(server.url("config_revisions_race"));
    tokio::task::block_in_place(|| -> TestResult {
        let seed = PostgresRevisions::open(&url)?;
        let Saved::Stored(_) = seed.save(None, draft("a = 0\n"))? else {
            return Err("the first save is stored".into());
        };
        let replicas = (0..RACERS)
            .map(|_| PostgresRevisions::open(&url))
            .collect::<Result<Vec<_>, _>>()?;
        let start = Arc::new(Barrier::new(RACERS));
        let outcomes = std::thread::scope(|scope| {
            let racing: Vec<_> = replicas
                .iter()
                .enumerate()
                .map(|(index, replica)| {
                    let start = Arc::clone(&start);
                    scope.spawn(move || {
                        start.wait();
                        replica.save(Some(RevisionId::FIRST), draft(&format!("a = {index}\n")))
                    })
                })
                .collect();
            racing
                .into_iter()
                .map(|racer| racer.join().map_err(|_panicked| "a racer panicked"))
                .collect::<Result<Vec<_>, _>>()
        })?;
        let stored = outcomes
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|saved| matches!(saved, Saved::Stored(_)))
            .count();
        assert_eq!(1, stored, "one base, one revision");
        assert_eq!(Some(id(2)?), seed.head()?);
        assert_eq!(2, seed.history()?.len());
        Ok(())
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_apply_on_one_gateway_reaches_the_other_through_the_shared_database() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres("config_revisions_watch", &[]).await?;
    let url = SecretUrl::new(server.url("config_revisions_watch"));
    let documents = tempfile::tempdir()?;
    std::fs::write(documents.path().join("one.toml"), member("a"))?;
    let two_members = documents.path().join("two.toml");
    std::fs::write(&two_members, member("a") + &member("b"))?;
    let (one, two) = tokio::task::block_in_place(|| -> Result<_, Box<dyn std::error::Error>> {
        let one = Gateway::build(
            "",
            Some((Box::new(PostgresRevisions::open(&url)?), documents.path())),
        )?;
        let two = Gateway::build(
            "",
            Some((Box::new(PostgresRevisions::open(&url)?), documents.path())),
        )?;
        Ok((one, two))
    })?;
    let watching = watch::start(
        Some(&two.revisions),
        &two.reloader,
        &two.state,
        Duration::from_millis(100),
    )
    .ok_or("a managed gateway watches its store")?;
    let (status, text) = one
        .save(Some(1), "add member B", &rest(&two_members, ""))
        .await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let (status, text) = one.post("/admin/config/revisions/2/apply", None).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while two.members()? != 2 {
        if tokio::time::Instant::now() >= deadline {
            return Err("the second gateway runs member B within 10 s".into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let (status, text) = two.get("/admin/config").await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let running: Standing = serde_json::from_str(&text)?;
    assert_eq!(Some(2), running.running);
    assert!(running.in_step);
    watching.stop(Duration::from_secs(5)).await;
    Ok(())
}
