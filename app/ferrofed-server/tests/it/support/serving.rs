// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The addresses a `ferrofed serve` process bound, read from its JSON log.
//!
//! A test hands the process port 0 and reads the port the system gave it
//! from the line that starts the listener. A port a test binds and releases
//! for the process instead can be taken by another process in between. No
//! specification governs this: our own design.

use std::io::{self, BufRead as _, BufReader};
use std::net::SocketAddr;
use std::process::ChildStdout;
use std::sync::mpsc::{self, RecvTimeoutError, SendError};
use std::time::{Duration, Instant};

use super::LogLine;

/// How long a process may take to start a listener.
const STARTUP: Duration = Duration::from_secs(30);

/// The log message of the gateway's own listener.
pub(crate) const GATEWAY: &str = "listening";

/// The log message of the admin listener.
pub(crate) const ADMIN: &str = "serving the admin listener";

/// Returns the address in the `listen` field of the line a `serve` process
/// logs with `message` on `stdout`, read as JSON lines
/// (`[telemetry] format = "json"`).
///
/// A thread reads `stdout` to its end, so the process never blocks on a full
/// pipe after the line is found, and the wait runs off the test's runtime.
///
/// # Errors
///
/// A process that stops, or logs no such line within [`STARTUP`], a line
/// that is no JSON log line, or a `listen` field that is missing or no
/// socket address.
pub(crate) async fn bound(stdout: ChildStdout, message: &'static str) -> io::Result<SocketAddr> {
    let (sender, lines) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            // A read error ends the log, which the waiting side reports as the
            // process having stopped.
            let Ok(line) = line else { return };
            // A line nobody waits for any more is read and left.
            let _unread: Result<(), SendError<String>> = sender.send(line);
        }
    });
    tokio::task::spawn_blocking(move || wait(&lines, message))
        .await
        .map_err(io::Error::other)?
}

/// Reads `lines` until the one logged with `message`, and returns the address
/// it names.
fn wait(lines: &mpsc::Receiver<String>, message: &str) -> io::Result<SocketAddr> {
    let deadline = Instant::now() + STARTUP;
    let mut seen = Vec::new();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let line = match lines.recv_timeout(left) {
            Ok(line) => line,
            Err(RecvTimeoutError::Timeout) => {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("no {message:?} line within {STARTUP:?}: {seen:?}"),
                ));
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!("the process stopped before {message:?}: {seen:?}"),
                ));
            }
        };
        let event: LogLine = serde_json::from_str(&line).map_err(io::Error::other)?;
        if event.message == message {
            let listen = event.listen.ok_or_else(|| {
                io::Error::other(format!("the {message:?} line names no listen address"))
            })?;
            return listen.parse().map_err(io::Error::other);
        }
        seen.push(line);
    }
}
