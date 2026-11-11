// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The keys of the gateway's own sections a release retired, refused for
//! the one release that retires them.
//!
//! Each section keeps a retired key as a
//! [`Removed`](ferrohealth_config::removed::Removed) field and this module
//! declares it once as a [`Retired`](ferrohealth_config::removed::Retired),
//! with the release that retired it.
//! [`check`] refuses every one the sources set, all at once, each naming
//! its replacement. The release after it deletes the field and the
//! declaration, and the strict load refuses the key as unknown. No
//! specification governs this: the family's rule of 2026-11-09, read through
//! `ferrohealth_config::removed`.

#[cfg(test)]
use ferrohealth_config::removed::Retired;
use ferrohealth_config::removed::{Removals, RemovedKeys};

use crate::config::Config;

/// The release the workspace is preparing.
///
/// `removed::stale` holds every declaration to it
/// (FerroHEALTH/FerroSYS#339). The workspace version moves only at the cut,
/// so the first pull request after each cut moves this on.
pub const PREPARING: &str = "0.0.20";

/// Every declaration of this module, for the one-release check.
#[cfg(test)]
const ALL: &[Retired] = &[];

/// Refuses every retired key `config` sets, all at once.
///
/// # Errors
///
/// [`RemovedKeys`] naming each retired key the sources set with its
/// replacement.
pub fn check(_config: &Config) -> Result<(), RemovedKeys> {
    Removals::new().finish()
}

#[cfg(test)]
mod tests {
    use ferrohealth_config::removed::stale;

    use super::{ALL, PREPARING};

    #[test]
    fn every_refusal_lives_for_the_one_release_that_retires_it() {
        let stale: Vec<_> = stale(ALL, PREPARING)
            .into_iter()
            .map(|retired| (retired.key(), retired.since()))
            .collect();
        assert!(stale.is_empty(), "delete these refusals: {stale:?}");
    }
}
