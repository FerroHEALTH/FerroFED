// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The forwarder: every message is stored first and delivered from the
//! spool in order, and a repository that cannot be reached leaves the
//! messages stored (ITI TF-2 §3.20.4.1.1).

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use ihe_iti::atna::forwarder::Forwarder;
use ihe_iti::atna::repository::Repository;
use ihe_iti::atna::spool::{Bounds, Spool};
use tokio::io::AsyncReadExt as _;
use tokio::net::TcpListener;
use url::Url;

use super::message;

/// The RFC 5425 frame of `text`: its octet count, a space, and `text`.
fn framed(text: &str) -> secrecy::SecretSlice<u8> {
    message(&format!("{} {text}", text.len()))
}

const BOUNDS: Bounds = Bounds {
    max_messages: 16,
    max_bytes: 4096,
};

/// A plain TCP repository on a free loopback port that keeps the octet-
/// counted messages it reads.
async fn repository() -> (Url, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a listener");
    let address = listener.local_addr().expect("an address");
    let kept = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&kept);
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let sink = Arc::clone(&sink);
            tokio::spawn(async move {
                let mut bytes = Vec::new();
                let _read = stream.read_to_end(&mut bytes).await;
                let mut rest = bytes.as_slice();
                while let Some(space) = rest.iter().position(|byte| *byte == b' ') {
                    let length: usize = std::str::from_utf8(&rest[..space])
                        .expect("a length")
                        .parse()
                        .expect("a number");
                    let end = space + 1 + length;
                    sink.lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(String::from_utf8(rest[space + 1..end].to_vec()).expect("UTF-8"));
                    rest = &rest[end..];
                }
            });
        }
    });
    let url = Url::parse(&format!("tcp://{address}")).expect("a URL");
    (url, kept)
}

#[tokio::test]
async fn stored_messages_are_delivered_in_order_and_leave_the_spool() {
    let (url, kept) = repository().await;
    let repository = Repository::unencrypted_for_development(&url, Duration::from_secs(2))
        .expect("a repository");
    let forwarder = Forwarder::new(Spool::in_memory(BOUNDS), repository);
    for text in ["one", "two", "three"] {
        forwarder.submit(framed(text)).await.expect("stored");
    }
    let running = tokio::spawn(Arc::clone(&forwarder).run());
    let until = Instant::now() + Duration::from_secs(5);
    while forwarder.status().delivered < 3 && Instant::now() < until {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    running.abort();
    let status = forwarder.status();
    assert_eq!(3, status.delivered);
    assert_eq!(0, status.depth.messages);
    assert!(status.reachable);
    // NOTE: the repository reads a connection to its end, so the messages are
    // read once the aborted forwarder's connection is closed.
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let messages = kept.lock().unwrap_or_else(PoisonError::into_inner).clone();
        if messages.len() == 3 || Instant::now() > until {
            assert_eq!(vec!["one", "two", "three"], messages);
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn a_repository_that_cannot_be_reached_leaves_every_message_stored() {
    let url = Url::parse("tcp://127.0.0.1:0").expect("a URL");
    let repository = Repository::unencrypted_for_development(&url, Duration::from_millis(500))
        .expect("a repository");
    let forwarder = Forwarder::new(Spool::in_memory(BOUNDS), repository);
    let running = tokio::spawn(Arc::clone(&forwarder).run());
    for text in ["kept", "also kept"] {
        forwarder.submit(framed(text)).await.expect("stored");
    }
    let until = Instant::now() + Duration::from_secs(5);
    while forwarder.status().reachable && Instant::now() < until {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    running.abort();
    let status = forwarder.status();
    assert!(!status.reachable);
    assert_eq!(0, status.delivered);
    assert_eq!(2, status.depth.messages, "nothing is dropped");
}

#[test]
fn an_address_of_another_form_is_refused() {
    let timeout = Duration::from_secs(1);
    for address in [
        "https://arr.example.org",
        "tls://arr.example.org/path",
        "tls://user@arr.example.org",
    ] {
        let url = Url::parse(address).expect("a URL");
        assert!(
            Repository::tls(
                &url,
                &ihe_iti::atna::repository::TlsSettings::default(),
                timeout
            )
            .is_err(),
            "{address}"
        );
    }
    let plain = Url::parse("tcp://arr.example.org").expect("a URL");
    assert!(
        Repository::unencrypted_for_development(&plain, timeout).is_err(),
        "a plain repository names its port"
    );
    let tls = Url::parse("tcp://arr.example.org:601").expect("a URL");
    assert!(
        Repository::tls(
            &tls,
            &ihe_iti::atna::repository::TlsSettings::default(),
            timeout
        )
        .is_err(),
        "tls:// only"
    );
}
