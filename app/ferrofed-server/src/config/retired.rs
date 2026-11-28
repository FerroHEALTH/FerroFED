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
/// so the first pull request after each cut moves this on, and
/// `preparing_names_the_coming_release` fails until it does.
pub const PREPARING: &str = "0.0.23";

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
mod book;

#[cfg(test)]
mod tests {
    use ferrohealth_config::removed::stale;

    use super::{ALL, PREPARING};

    /// Returns the three numbers of the release `version`.
    fn release(version: &str) -> [u64; 3] {
        let numbers: Vec<u64> = version.split('.').map(|n| n.parse().unwrap()).collect();
        numbers.try_into().unwrap()
    }

    /// Whether a changelog fragment is pending under `changelog.d/`.
    fn fragment_pending() -> bool {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../changelog.d");
        std::fs::read_dir(dir).unwrap().any(|entry| {
            let name = entry.unwrap().file_name();
            let name = name.to_string_lossy();
            name.ends_with(".md") && name != "README.md"
        })
    }

    // NOTE: no specification governs this: our own design; the release window
    // holds no fragment, so one pending at the workspace version means the cut is past.
    #[test]
    fn preparing_names_the_coming_release() {
        let (preparing, workspace) = (release(PREPARING), release(env!("CARGO_PKG_VERSION")));
        assert!(
            preparing > workspace || (preparing == workspace && !fragment_pending()),
            "PREPARING is {PREPARING} with the workspace at {} and fragments pending; \
             move it to the release being prepared",
            env!("CARGO_PKG_VERSION")
        );
    }

    #[test]
    fn every_refusal_lives_for_the_one_release_that_retires_it() {
        let stale: Vec<_> = stale(ALL, PREPARING)
            .into_iter()
            .map(|retired| (retired.key(), retired.since()))
            .collect();
        assert!(stale.is_empty(), "delete these refusals: {stale:?}");
    }
}
