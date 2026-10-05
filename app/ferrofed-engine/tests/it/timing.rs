// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The waits a test of a deadline makes, sized so that how busy the host is
//! cannot change which side of the deadline an event falls on.
//!
//! A node meant to stay silent delays its answer past the life of any test,
//! so the deadline is the only bound on the wait. A deadline is taken after
//! the client is built and sits [`SLACK`] past the moment it is taken, so the
//! request always leaves before it and a node meant to answer answers before
//! it. No specification governs this: our own design.

use std::error::Error;
use std::time::{Duration, Instant};

/// The delay of a node that never answers inside a test.
pub(crate) const SILENT: Duration = Duration::from_hours(1);

/// The time a loaded host may add to any step a test waits on.
pub(crate) const SLACK: Duration = Duration::from_secs(3);

/// The instant [`SLACK`] from now.
///
/// # Errors
///
/// Returns an error when the platform clock cannot represent the instant.
pub(crate) fn deadline() -> Result<Instant, Box<dyn Error>> {
    Ok(Instant::now()
        .checked_add(SLACK)
        .ok_or("the deadline is past the platform clock")?)
}
