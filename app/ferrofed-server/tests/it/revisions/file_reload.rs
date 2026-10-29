// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A `SIGHUP` reload of a changed file on one of two replicas over one
//! store, and a start of a third over it: the file's revision is marked
//! running once the gateway runs it, and the other replica follows it; a
//! revision the reload or the start refuses is recorded as refused, the
//! running mark stays, and the other replica keeps its configuration. No
//! specification governs this: our own design.

use std::path::Path;

use crate::support::console::subscriber;
use ferrofed_registry::revision::{Author, Origin, RevisionId};
use ferrofed_server::config::Config;
use ferrofed_server::reload::ReloadError;
use ferrofed_server::revision::service::{self, Booted, Followed, REFUSED};
use ferrohealth_telemetry::section::Rendering;

use super::{Replicas, TestResult};
use crate::support::Logs;

/// The line of the file's `[registry]` table that names its document.
fn registry_lines(text: &str) -> Option<String> {
    let start = text.find("[registry]\n")?;
    let line = text.get(start..)?.lines().take(2).collect::<Vec<_>>();
    Some(line.join("\n") + "\n")
}

#[test]
fn a_reload_that_refuses_the_file_leaves_the_mark_and_the_other_replica_alone() -> TestResult {
    let replicas = Replicas::start()?;
    let one = &replicas.one;
    let text = std::fs::read_to_string(&one.config)?;
    let registry = registry_lines(&text).ok_or("the file names its registry document")?;
    // Unsetting the registry document takes a restart, so the reload refuses
    // a file revision that checks as any start would.
    std::fs::write(&one.config, text.replace(&registry, ""))?;

    let logs = Logs::default();
    let capture = subscriber(Rendering::Json, "info", false, logs.clone())?;
    let reloaded = tracing::subscriber::with_default(capture, || one.reloader.reload());
    let refused = reloaded.err().ok_or("the reload is refused")?;
    assert!(
        matches!(refused, ReloadError::RegistryPresence),
        "{refused:?}"
    );

    let store = one.revisions.store();
    assert_eq!(
        Some(2),
        store.head()?.map(RevisionId::get),
        "the file's load is recorded"
    );
    let recorded = store
        .revision(RevisionId::new(2).ok_or("2")?)?
        .ok_or("held")?;
    assert_eq!(Origin::File, recorded.origin);
    let current = store.current()?.ok_or("a mark")?;
    assert_eq!(
        RevisionId::new(1),
        Some(current.revision),
        "the refused revision is not marked running"
    );
    assert_eq!(1, one.members()?, "the running configuration stays");
    assert_eq!(RevisionId::new(1), one.revisions.replica().running);

    let text = logs.text();
    let line = text
        .lines()
        .find(|line| line.contains(REFUSED))
        .ok_or_else(|| format!("the refusal is recorded: {text}"))?;
    assert!(line.contains("registry-presence"), "{line}");
    assert!(line.contains("\"revision\":\"2\""), "{line}");

    assert_eq!(
        Followed::InStep,
        replicas.two.revisions.follow(&replicas.two.reloader)?,
        "the other replica does not follow a revision the reload refused"
    );
    assert_eq!(RevisionId::new(1), replicas.two.revisions.replica().running);
    assert_eq!(1, replicas.two.members()?);

    // The same file read again is tried again, and refused again.
    assert!(one.reloader.reload().is_err());
    assert_eq!(
        RevisionId::new(1),
        store.current()?.map(|current| current.revision),
        "a second refusal leaves the mark too"
    );

    // A file fixed again runs, marked running from the mark the load read.
    std::fs::write(
        &one.config,
        std::fs::read_to_string(&one.config)? + &registry,
    )?;
    one.reloader.reload()?;
    let current = store.current()?.ok_or("a mark")?;
    let head = store.head()?.ok_or("a head")?;
    assert_eq!(head, current.revision, "the fixed file's revision runs");
    assert_eq!(RevisionId::new(1), current.previous);
    assert_eq!(Author::File, current.applied_by);
    assert_eq!(
        Followed::Applied(head),
        replicas.two.revisions.follow(&replicas.two.reloader)?
    );
    Ok(())
}

#[test]
fn a_reload_that_applies_the_file_marks_it_running_and_the_other_replica_follows() -> TestResult {
    let replicas = Replicas::start()?;
    let one = &replicas.one;
    let text = std::fs::read_to_string(&one.config)?;
    let document = toml::Value::String(replicas.two_members().display().to_string());
    let registry = registry_lines(&text).ok_or("the file names its registry document")?;
    std::fs::write(
        &one.config,
        text.replace(&registry, &format!("[registry]\ndocument = {document}\n")),
    )?;

    one.reloader.reload()?;
    let store = one.revisions.store();
    let current = store.current()?.ok_or("a mark")?;
    assert_eq!(RevisionId::new(2), Some(current.revision));
    assert_eq!(RevisionId::new(1), current.previous);
    assert_eq!(Author::File, current.applied_by);
    assert_eq!(2, one.members()?);
    assert_eq!(RevisionId::new(2), one.revisions.replica().running);

    assert_eq!(
        Followed::Applied(RevisionId::new(2).ok_or("2")?),
        replicas.two.revisions.follow(&replicas.two.reloader)?
    );
    assert_eq!(2, replicas.two.members()?);
    Ok(())
}

/// Starts a third gateway over the store of `replicas`, its file the first
/// replica's with the registry document `document`, as far as the revisions
/// take it: the file recorded, the gateway not yet running it.
fn third_start(replicas: &Replicas, document: &Path) -> Result<Booted, Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(&replicas.one.config)?;
    let registry = registry_lines(&text).ok_or("the file names its registry document")?;
    let document = toml::Value::String(document.display().to_string());
    let dir = tempfile::tempdir()?;
    let config = dir.path().join("ferrofed.toml");
    std::fs::write(
        &config,
        text.replace(&registry, &format!("[registry]\ndocument = {document}\n")),
    )?;
    let (_, tree) = Config::load_with_tree(Some(&config))?;
    let (_, booted) = service::boot_over(Box::new(replicas.shared.clone()), &tree)?;
    Ok(booted)
}

#[test]
fn a_start_whose_file_builds_nothing_leaves_the_mark_and_the_other_replica_alone() -> TestResult {
    let replicas = Replicas::start()?;
    let missing = replicas.documents.path().join("missing.toml");
    assert!(
        third_start(&replicas, &missing).is_err(),
        "a start over a registry document that does not read is refused"
    );
    let store = replicas.one.revisions.store();
    assert_eq!(Some(1), store.head()?.map(RevisionId::get));
    assert_eq!(
        RevisionId::new(1),
        store.current()?.map(|current| current.revision)
    );
    assert_eq!(
        Followed::InStep,
        replicas.two.revisions.follow(&replicas.two.reloader)?
    );
    Ok(())
}

#[test]
fn a_start_that_fails_after_recording_the_file_leaves_the_mark_and_records_the_refusal()
-> TestResult {
    let replicas = Replicas::start()?;
    let booted = third_start(&replicas, &replicas.two_members())?;
    assert!(booted.file.recorded);
    assert_eq!(RevisionId::new(2), Some(booted.file.revision));

    // The state the start builds next fails, as a listener or a binding may.
    let logs = Logs::default();
    let capture = subscriber(Rendering::Json, "info", false, logs.clone())?;
    tracing::subscriber::with_default(capture, || {
        booted.refused(
            "state",
            "what the configuration describes could not be built",
        );
    });

    let store = replicas.one.revisions.store();
    assert_eq!(
        RevisionId::new(1),
        store.current()?.map(|current| current.revision),
        "a start that failed marks nothing"
    );
    let text = logs.text();
    let line = text
        .lines()
        .find(|line| line.contains(REFUSED))
        .ok_or_else(|| format!("the refusal is recorded: {text}"))?;
    assert!(line.contains("\"revision\":\"2\""), "{line}");
    assert_eq!(
        Followed::InStep,
        replicas.two.revisions.follow(&replicas.two.reloader)?,
        "the other replica does not follow a revision a start refused"
    );
    assert_eq!(1, replicas.two.members()?);
    Ok(())
}

#[test]
fn a_start_marks_the_file_revision_once_it_runs_and_the_other_replica_follows() -> TestResult {
    let replicas = Replicas::start()?;
    let booted = third_start(&replicas, &replicas.two_members())?;
    let store = replicas.one.revisions.store();
    assert_eq!(
        RevisionId::new(1),
        store.current()?.map(|current| current.revision),
        "the file's revision is not marked before the gateway runs it"
    );
    booted.started()?;
    let current = store.current()?.ok_or("a mark")?;
    assert_eq!(RevisionId::new(2), Some(current.revision));
    assert_eq!(RevisionId::new(1), current.previous);
    assert_eq!(Author::File, current.applied_by);
    assert_eq!(
        Followed::Applied(RevisionId::new(2).ok_or("2")?),
        replicas.two.revisions.follow(&replicas.two.reloader)?
    );
    assert_eq!(2, replicas.two.members()?);
    Ok(())
}
