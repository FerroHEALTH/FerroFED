// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A `SIGHUP` reload of a changed file on one of two replicas over one
//! store: the file's revision is marked running once the reload applied it,
//! and the other replica follows it; a revision the reload refuses is
//! recorded as refused, the running mark stays, and the other replica keeps
//! its configuration. No specification governs this: our own design.

use ferrofed_registry::revision::{Author, Origin, RevisionId};
use ferrofed_server::reload::ReloadError;
use ferrofed_server::revision::service::{Followed, REFUSED};
use ferrofed_server::telemetry::{Rendering, subscriber};

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
