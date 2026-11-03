// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The in-process OTLP collector stops when it is dropped, whatever its
//! clients do (#1507).

use std::error::Error;
use std::io::{Read as _, Write as _};
use std::net::TcpStream;
use std::sync::mpsc;
use std::time::Duration;

use ferrofed_testkit::otlp::Collector;

type TestResult = Result<(), Box<dyn Error>>;

/// The HTTP/2 client connection preface and an empty `SETTINGS` frame
/// (RFC 9113 §3.4, §6.5).
const PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n\0\0\0\x04\0\0\0\0\0";

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_dropped_collector_stops_while_a_client_holds_its_connection_open() -> TestResult {
    let collector = Collector::start(None)?;
    let address = collector
        .endpoint()
        .strip_prefix("http://")
        .ok_or("the collector serves http")?
        .to_owned();
    let mut client = TcpStream::connect(address)?;
    client.set_read_timeout(Some(Duration::from_secs(5)))?;
    client.write_all(PREFACE)?;
    let mut settings = [0_u8; 9];
    client.read_exact(&mut settings)?;
    assert_eq!(settings[3], 0x04, "the collector answers with its SETTINGS");
    let (dropped, stopped) = mpsc::channel();
    std::thread::spawn(move || {
        drop(collector);
        let _sent: Result<(), mpsc::SendError<()>> = dropped.send(());
    });
    assert!(
        stopped.recv_timeout(Duration::from_secs(10)).is_ok(),
        "the collector stops with the client's connection still open"
    );
    drop(client);
    Ok(())
}
