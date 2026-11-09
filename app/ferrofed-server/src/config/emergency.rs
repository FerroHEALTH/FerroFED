// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[emergency]` table: how many emergency accesses one health
//! professional a national contact point relays may make in one window.
//!
//! eHDSI Section II Security Services 10.0.0 §2.5 has the contact point of
//! the patient's country store the identity of the professional of each
//! emergency access and deny the access "after a number of accesses made by
//! the same HP identity in a defined amount of time", and sets the baseline:
//! "No more than five emergency access attempts per HP per 24-hour period",
//! which each country may tighten and "SHALL" define. The `NCPeH` Architecture
//! Specifications 10.0.0 §5.1.9 make a professional "exceeding the
//! country-defined threshold" a deny response. Implementing Regulation (EU)
//! 2026/2083 Art 4(1) binds the national contact points to these
//! specifications; applying them at the gateway behind the contact point is
//! the deployment's national implementation, our own design.
//!
//! `threshold` is how many emergency accesses one professional is granted in
//! any `window`. The defaults are the §2.5 baseline, and a value
//! that loosens it is refused: a country may only tighten the limit. A
//! threshold of zero is refused too, because §2.5 has every country provide
//! emergency access.

use ferrohealth_config::removed::Removed;
use ferrohealth_config::units;
use std::num::NonZeroU32;
use std::time::Duration;

use serde::Deserialize;

use crate::config::error::Error;

/// The most emergency accesses one professional may be granted in one
/// window: the eHDSI Section II Security Services 10.0.0 §2.5 baseline.
pub const MAX_THRESHOLD: u32 = 5;

/// The longest window, in seconds: the 24-hour period of eHDSI Section II
/// Security Services 10.0.0 §2.5.
pub const MAX_WINDOW_S: u64 = 24 * 60 * 60;

/// `[emergency]` as written.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Emergency {
    /// How many emergency accesses one professional is granted in one
    /// window, 1 to [`MAX_THRESHOLD`].
    pub threshold: u32,
    /// The window, in seconds, 1 to [`MAX_WINDOW_S`].
    pub window: units::Duration,
    /// The retired `window_s`, which `window` replaced in 0.0.18: set, it
    /// refuses the configuration.
    #[serde(skip_serializing)]
    pub window_s: Removed,
}

impl Default for Emergency {
    fn default() -> Self {
        Self {
            threshold: MAX_THRESHOLD,
            window: units::Duration::from_secs(MAX_WINDOW_S),
            window_s: Removed::default(),
        }
    }
}

/// `[emergency]`, resolved: the limit every relayed professional's
/// emergency accesses are held to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmergencyLimit {
    /// How many emergency accesses one professional is granted in one
    /// window.
    pub threshold: NonZeroU32,
    /// The window: any span of this length holds at most `threshold`
    /// granted accesses of one professional.
    pub window: Duration,
}

impl Default for EmergencyLimit {
    fn default() -> Self {
        Self {
            threshold: NonZeroU32::MIN.saturating_add(MAX_THRESHOLD - 1),
            window: Duration::from_secs(MAX_WINDOW_S),
        }
    }
}

/// An `[emergency]` limit looser than the eHDSI baseline.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum EmergencyFault {
    /// `threshold` is above the baseline.
    #[error(
        "emergency.threshold is {value}, above the baseline of {MAX_THRESHOLD} emergency access attempts per health professional per 24-hour period of eHDSI Section II Security Services 10.0.0 §2.5, which a country may only tighten"
    )]
    Threshold {
        /// The value written.
        value: u32,
    },
    /// `window` is longer than the baseline's 24 hours.
    #[error(
        "emergency.window is {value} s, longer than the 24-hour period ({MAX_WINDOW_S} s) of eHDSI Section II Security Services 10.0.0 §2.5, which a country may only tighten"
    )]
    Window {
        /// The value written.
        value: u64,
    },
}

/// Resolves `[emergency]`.
///
/// # Errors
///
/// [`EmergencyFault::Threshold`] for a threshold above [`MAX_THRESHOLD`] and
/// [`EmergencyFault::Window`] for a window above [`MAX_WINDOW_S`], each
/// naming §2.5; [`Error::Zero`] for a threshold or a window of zero.
pub fn resolve(table: &Emergency) -> Result<EmergencyLimit, Error> {
    if table.threshold > MAX_THRESHOLD {
        return Err(EmergencyFault::Threshold {
            value: table.threshold,
        }
        .into());
    }
    if table.window.as_secs() > MAX_WINDOW_S {
        return Err(EmergencyFault::Window {
            value: table.window.as_secs(),
        }
        .into());
    }
    // NOTE: eHDSI Section II Security Services 10.0.0 §2.5: every country "MUST provide"
    // emergency access, so a limit that grants none is refused.
    let threshold = NonZeroU32::new(table.threshold).ok_or_else(|| Error::Zero {
        key: String::from("emergency.threshold"),
    })?;
    if table.window.as_secs() == 0 {
        return Err(Error::Zero {
            key: String::from("emergency.window"),
        });
    }
    Ok(EmergencyLimit {
        threshold,
        window: Duration::from_secs(table.window.as_secs()),
    })
}

#[cfg(test)]
mod tests {
    use super::{Emergency, EmergencyLimit, resolve};

    #[test]
    fn the_default_table_resolves_to_the_default_limit() {
        let resolved = resolve(&Emergency::default()).map_err(|error| error.to_string());
        assert_eq!(Ok(EmergencyLimit::default()), resolved);
        assert_eq!(5, EmergencyLimit::default().threshold.get());
    }
}
