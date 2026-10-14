// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The waits a test of the audit forwarder makes, sized so that how busy the
//! host is cannot change which side of a deadline an event falls on.
//!
//! The forwarder reaches the harness repository over real TLS connections,
//! so the tokio clock cannot be paused under it: a paused clock advances to
//! the next timer whenever the runtime is idle, even while a handshake is
//! still in flight. A wait starts after the forwarder and its repository are
//! set up, and lasts what the forwarder's own timeouts allow plus [`SLACK`].
//! A wait for a delivery waits on the delivery itself and is bounded only
//! by [`HANG`]. No specification governs this: our own design.

use std::time::Duration;

/// The time a loaded host may add to any step a test waits on.
pub(crate) const SLACK: Duration = Duration::from_secs(3);

/// The wait of `waits` plus [`SLACK`].
pub(crate) const fn within(waits: Duration) -> Duration {
    waits.saturating_add(SLACK)
}

/// The longest a test waits for an event before it fails and names what it
/// waited for. It guards against a hang and holds the code to no budget,
/// and it stays under the 60 seconds after which nextest kills a test.
pub(crate) const HANG: Duration = Duration::from_secs(45);
