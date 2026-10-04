// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The spool: bounded, ordered, durable on disk, and private to its owner.

use ihe_iti::atna::spool::{Bounds, Depth, Spool, SpoolError};
use secrecy::ExposeSecret;

use super::message;

const ROOMY: Bounds = Bounds {
    max_messages: 16,
    max_bytes: 4096,
};

async fn drain(spool: &Spool) -> Vec<String> {
    let mut texts = Vec::new();
    while let Some(stored) = spool.oldest().await.expect("the spool reads") {
        texts.push(String::from_utf8(stored.message.expose_secret().to_vec()).expect("UTF-8"));
        spool
            .remove(stored.sequence)
            .await
            .expect("the spool removes");
    }
    texts
}

#[tokio::test]
async fn messages_leave_in_the_order_they_were_stored() {
    let directory = tempfile::tempdir().expect("a directory");
    let spool = Spool::open(&directory.path().join("spool"), ROOMY).expect("the spool opens");
    for text in ["first", "second", "third"] {
        spool.push(message(text)).await.expect("stored");
    }
    assert_eq!(
        Depth {
            messages: 3,
            bytes: 16
        },
        spool.depth()
    );
    assert_eq!(vec!["first", "second", "third"], drain(&spool).await);
    assert_eq!(Depth::default(), spool.depth());
}

#[tokio::test]
async fn a_stored_message_survives_a_restart() {
    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("spool");
    {
        let spool = Spool::open(&path, ROOMY).expect("the spool opens");
        spool.push(message("kept")).await.expect("stored");
        spool.push(message("also kept")).await.expect("stored");
    }
    let reopened = Spool::open(&path, ROOMY).expect("the spool reopens");
    assert_eq!(2, reopened.depth().messages);
    reopened.push(message("after")).await.expect("stored");
    assert_eq!(vec!["kept", "also kept", "after"], drain(&reopened).await);
}

#[tokio::test]
async fn a_message_past_either_bound_is_refused_and_nothing_is_dropped() {
    let directory = tempfile::tempdir().expect("a directory");
    let by_count = Spool::open(
        &directory.path().join("count"),
        Bounds {
            max_messages: 2,
            max_bytes: 4096,
        },
    )
    .expect("the spool opens");
    by_count.push(message("one")).await.expect("stored");
    by_count.push(message("two")).await.expect("stored");
    assert!(matches!(
        by_count.push(message("three")).await,
        Err(SpoolError::Full { messages: 2, .. })
    ));
    assert_eq!(vec!["one", "two"], drain(&by_count).await);

    let by_size = Spool::in_memory(Bounds {
        max_messages: 16,
        max_bytes: 8,
    });
    by_size.push(message("12345")).await.expect("stored");
    assert!(matches!(
        by_size.push(message("6789")).await,
        Err(SpoolError::Full { bytes: 5, .. })
    ));
    assert_eq!(vec!["12345"], drain(&by_size).await);
}

#[cfg(unix)]
#[tokio::test]
async fn the_directory_and_every_message_are_private_to_their_owner() {
    use std::os::unix::fs::PermissionsExt as _;

    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("spool");
    let spool = Spool::open(&path, ROOMY).expect("the spool opens");
    spool.push(message("private")).await.expect("stored");
    let mode = |path: &std::path::Path| {
        std::fs::metadata(path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777
    };
    assert_eq!(0o700, mode(&path));
    for entry in std::fs::read_dir(&path).expect("the directory reads") {
        assert_eq!(0o600, mode(&entry.expect("an entry").path()));
    }

    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o750)).expect("chmod");
    assert!(matches!(
        Spool::open(&path, ROOMY),
        Err(SpoolError::Exposed(_))
    ));
}

#[cfg(unix)]
#[test]
fn a_directory_the_owner_cannot_write_is_refused_when_opened() {
    use std::os::unix::fs::PermissionsExt as _;

    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("spool");
    drop(Spool::open(&path, ROOMY).expect("the spool opens"));
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o500)).expect("chmod");
    let refused = Spool::open(&path, ROOMY);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).expect("chmod");
    assert!(
        matches!(
            refused,
            Err(SpoolError::Io {
                action: "created",
                ..
            })
        ),
        "{refused:?}"
    );
}

#[tokio::test]
async fn a_directory_holding_another_file_is_refused_and_a_partial_write_is_dropped() {
    let directory = tempfile::tempdir().expect("a directory");
    let path = directory.path().join("spool");
    let spool = Spool::open(&path, ROOMY).expect("the spool opens");
    spool.push(message("whole")).await.expect("stored");
    drop(spool);
    std::fs::write(path.join("00000000000000000007.partial"), b"torn").expect("a partial");
    let reopened = Spool::open(&path, ROOMY).expect("the spool reopens");
    assert_eq!(vec!["whole"], drain(&reopened).await);

    std::fs::write(path.join("notes.txt"), b"not a message").expect("a stray file");
    assert!(matches!(
        Spool::open(&path, ROOMY),
        Err(SpoolError::Foreign(_))
    ));
}
