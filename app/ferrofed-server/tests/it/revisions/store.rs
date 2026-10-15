// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The embedded revision store: numbering from one, a save on a stale base
//! refused with nothing stored, the current mark moved only from where the
//! writer read it, everything held across a reopen, and a file a newer
//! FerroFED wrote refused. No specification governs this: our own design.

use ferrofed_server::revision::embedded::{LAYOUT, RedbRevisions, VERSION};
use ferrofed_server::revision::{
    Author, Current, Draft, Origin, Outcome, RevisionId, RevisionStore, Saved, StoreError, Swapped,
};
use jiff::Timestamp;

use super::TestResult;

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

#[test]
fn a_save_on_a_stale_base_stores_nothing() -> TestResult {
    let dir = tempfile::tempdir()?;
    let store = RedbRevisions::open(&dir.path().join("revisions.redb"))?;
    assert_eq!(None, store.head()?);
    let Saved::Stored(first) = store.save(None, draft("a = 1\n"))? else {
        return Err("the first save is stored".into());
    };
    assert_eq!(RevisionId::FIRST, first.id);
    assert_eq!(None, first.base);
    assert_eq!(
        Saved::Stale {
            head: Some(RevisionId::FIRST)
        },
        store.save(None, draft("a = 2\n"))?
    );
    let Saved::Stored(second) = store.save(Some(RevisionId::FIRST), draft("a = 2\n"))? else {
        return Err("a save on the newest is stored".into());
    };
    assert_eq!(id(2)?, second.id);
    assert_eq!(
        Saved::Stale { head: Some(id(2)?) },
        store.save(Some(RevisionId::FIRST), draft("a = 3\n"))?,
        "two writers on one base: the second is refused"
    );
    assert_eq!(
        vec![first.clone(), second.clone()],
        store.history()?,
        "oldest first, and the refused saves left nothing"
    );
    assert_eq!(Some(second), store.revision(id(2)?)?);
    assert_eq!(None, store.revision(id(3)?)?);
    Ok(())
}

#[test]
fn the_mark_moves_only_from_where_the_writer_read_it() -> TestResult {
    let dir = tempfile::tempdir()?;
    let store = RedbRevisions::open(&dir.path().join("revisions.redb"))?;
    assert_eq!(None, store.current()?);
    assert_eq!(
        Swapped::Swapped,
        store.swap_current(None, &mark(RevisionId::FIRST, None))?
    );
    assert_eq!(
        Swapped::Stale {
            current: Some(RevisionId::FIRST)
        },
        store.swap_current(None, &mark(id(2)?, None))?
    );
    let next = mark(id(2)?, Some(RevisionId::FIRST));
    assert_eq!(
        Swapped::Swapped,
        store.swap_current(Some(RevisionId::FIRST), &next)?
    );
    assert_eq!(
        Swapped::Stale {
            current: Some(id(2)?)
        },
        store.swap_current(Some(RevisionId::FIRST), &mark(id(3)?, None))?
    );
    assert_eq!(Some(next), store.current()?);
    Ok(())
}

#[test]
fn everything_is_held_across_a_reopen() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("revisions.redb");
    {
        let store = RedbRevisions::open(&path)?;
        assert_eq!(path, store.path());
        let Saved::Stored(_) = store.save(None, draft("a = 1\n"))? else {
            return Err("stored".into());
        };
        let _swapped = store.swap_current(None, &mark(RevisionId::FIRST, None))?;
    }
    let store = RedbRevisions::open(&path)?;
    assert_eq!(Some(RevisionId::FIRST), store.head()?);
    assert_eq!(Some(mark(RevisionId::FIRST, None)), store.current()?);
    assert!(RedbRevisions::open(&path).is_err(), "one process at a time");
    Ok(())
}

#[test]
fn a_file_a_newer_ferrofed_wrote_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("revisions.redb");
    drop(RedbRevisions::open(&path)?);
    {
        let database = redb::Database::open(&path)?;
        let write = database.begin_write()?;
        write
            .open_table(redb::TableDefinition::<&str, u32>::new("ferrofed_schema"))?
            .insert(LAYOUT, VERSION + 1)?;
        write.commit()?;
    }
    let refused = RedbRevisions::open(&path).err().ok_or("refused")?;
    assert!(
        matches!(
            refused,
            StoreError::Newer { found, supported } if found == VERSION + 1 && supported == VERSION
        ),
        "{refused:?}"
    );
    Ok(())
}
