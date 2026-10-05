// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The unreachable base: no listener can hold it, and a connection to it
//! fails at once.

use std::error::Error;
use std::time::Duration;

use ferrofed_testkit::unreachable::{ADDRESS, BASE};

type TestResult = Result<(), Box<dyn Error>>;

#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn a_connection_to_the_unreachable_address_fails_at_once() -> TestResult {
    let connecting = tokio::net::TcpStream::connect(ADDRESS);
    let attempt = tokio::time::timeout(Duration::from_secs(2), connecting).await;
    let Ok(connected) = attempt else {
        return Err("the connection waited instead of failing".into());
    };
    assert!(connected.is_err(), "a connection was accepted");
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn no_listener_can_hold_the_unreachable_address() -> TestResult {
    let listener = std::net::TcpListener::bind(ADDRESS)?;
    assert_ne!(
        ADDRESS.port(),
        listener.local_addr()?.port(),
        "binding port 0 picks another port"
    );
    assert_eq!(format!("http://{ADDRESS}"), BASE);
    Ok(())
}
