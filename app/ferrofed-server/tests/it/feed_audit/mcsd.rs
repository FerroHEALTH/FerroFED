// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ITI-90 and ITI-91 audit records of the registry read from a care
//! services directory (mCSD §2:3.90.5.1, §2:3.91.5.1): the first read
//! records one ITI-90 search per resource type, each refresh one ITI-91
//! history per resource type, a repository that is down holds them, and a
//! read whose record the spool cannot take refuses the boot, as a directory
//! that did not answer does.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::time::Duration;

use ferrofed_testkit::atna_feed::FeedRepository;
use ferrofed_testkit::mcsd::HarnessDirectory;

use super::{audit_tables, spool_key, spooled, transactions};
use crate::registry_mcsd::{Gateway, config, members};

type TestResult = Result<(), Box<dyn Error>>;

/// The configuration of a gateway reading its registry from `harness`,
/// recording to `repository` with the `[audit.repository]` keys `extra`.
fn audited(harness: &HarnessDirectory, repository: &FeedRepository, extra: &str) -> String {
    format!(
        "{}{}",
        config(&harness.base(), 5_000),
        audit_tables(repository, extra)
    )
}

#[tokio::test]
async fn the_first_read_and_each_refresh_are_recorded() -> TestResult {
    let harness = HarnessDirectory::start().await;
    harness.publish(&members(
        "https://cdr-a.example.org/openehr",
        "https://cdr-b.example.org/openehr",
    ))?;
    let repository = FeedRepository::start().await;
    let gateway = Gateway::boot_from(&audited(&harness, &repository, ""))?;
    let read = repository.wait_for(2, Duration::from_secs(5)).await;
    assert_eq!(2, read.len(), "one ITI-90 search per resource type");
    for record in &read {
        assert_eq!(vec!["ITI-90"], transactions(record)?);
    }
    let _outcome = gateway.directory.refresh(&gateway.reloader).await;
    let all = repository.wait_for(4, Duration::from_secs(5)).await;
    assert_eq!(4, all.len(), "one ITI-91 history per resource type");
    for record in all.iter().skip(2) {
        assert_eq!(vec!["ITI-91"], transactions(record)?);
    }
    Ok(())
}

#[tokio::test]
async fn a_repository_that_is_down_holds_the_records_and_the_registry_is_read() -> TestResult {
    let harness = HarnessDirectory::start().await;
    harness.publish(&members(
        "https://cdr-a.example.org/openehr",
        "https://cdr-b.example.org/openehr",
    ))?;
    let repository = FeedRepository::start().await;
    repository.set_up(false);
    let dir = tempfile::tempdir()?;
    let (key, spool) = spool_key(dir.path());
    let gateway = Gateway::boot_from(&audited(&harness, &repository, &key))?;
    assert_eq!(2, gateway.addresses()?.len(), "the registry is read");
    assert_eq!(2, spooled(&spool)?, "both records are on disk");
    repository.set_up(true);
    let records = repository.wait_for(2, Duration::from_secs(10)).await;
    assert_eq!(
        2,
        records.len(),
        "the spool drains once the repository is back"
    );
    Ok(())
}

#[tokio::test]
async fn a_read_whose_record_the_spool_cannot_take_refuses_the_boot() -> TestResult {
    let harness = HarnessDirectory::start().await;
    harness.publish(&members(
        "https://cdr-a.example.org/openehr",
        "https://cdr-b.example.org/openehr",
    ))?;
    let repository = FeedRepository::start().await;
    repository.set_up(false);
    let refused = Gateway::boot_from(&audited(&harness, &repository, "spool_max_events = 1"));
    let error = refused
        .err()
        .ok_or("the second search's record is refused")?;
    let mut chain = error.to_string();
    let mut cause = error.source();
    while let Some(source) = cause {
        chain.push_str(": ");
        chain.push_str(&source.to_string());
        cause = source.source();
    }
    assert!(chain.contains("audit"), "{chain}");
    Ok(())
}
