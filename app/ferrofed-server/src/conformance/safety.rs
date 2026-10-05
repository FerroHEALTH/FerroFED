// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a conformance run refuses before it writes anything.
//!
//! A run writes synthetic EHRs, a template and compositions to the nodes,
//! and stored queries to the gateway's registry, and removes none of them.
//! So it refuses to start without [`ALLOW_WRITES`], refuses a deployment
//! whose profile is not `development` unless the operator also passes
//! [`ACKNOWLEDGE`], and refuses a patient outside the example arc. Each
//! refusal comes before the configuration's nodes are asked anything. No
//! specification governs the refusals: our own design.

use ferrofed_identity::dev::Profile;

use crate::conformance::fixture::{EXAMPLE_ARC, PatientError};

/// The flag a run refuses to start without.
pub const ALLOW_WRITES: &str = "--allow-writes";

/// The flag a run against a deployment whose profile is not `development`
/// refuses to start without.
pub const ACKNOWLEDGE: &str = "--i-understand-this-writes-synthetic-data-to-the-nodes";

/// Why a run refused to start.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Refusal {
    /// The operator did not allow the run's writes.
    #[error(
        "a conformance run writes to the nodes (a synthetic patient's EHR, the vendored template and compositions, EHRs with no subject) and to the gateway's stored-query registry, and removes none of it; pass {ALLOW_WRITES} to allow that"
    )]
    WritesNotAllowed,
    /// The deployment is not a development one and the operator did not
    /// acknowledge the writes.
    #[error(
        "the configuration's profile is not development, so the nodes may hold real records; a run writes only synthetic data in the {EXAMPLE_ARC} arc, and starts against such a deployment only with {ACKNOWLEDGE}"
    )]
    NotDevelopment,
    /// The patient is not one a run uses.
    #[error(transparent)]
    Patient(#[from] PatientError),
    /// The caller's token file holds no token.
    #[error("the token file holds no bearer token")]
    NoToken,
}

/// Refuses a run the operator has not allowed to write.
///
/// # Errors
///
/// Returns [`Refusal::WritesNotAllowed`] when `allow_writes` is false.
pub fn writes(allow_writes: bool) -> Result<(), Refusal> {
    if allow_writes {
        Ok(())
    } else {
        Err(Refusal::WritesNotAllowed)
    }
}

/// Refuses a run against a deployment of `profile` that is not
/// `development`, unless `acknowledged`.
///
/// # Errors
///
/// Returns [`Refusal::NotDevelopment`] for any other profile without the
/// acknowledgement.
pub fn profile(profile: Profile, acknowledged: bool) -> Result<(), Refusal> {
    if profile == Profile::Development || acknowledged {
        Ok(())
    } else {
        Err(Refusal::NotDevelopment)
    }
}

#[cfg(test)]
mod tests {
    use super::{Refusal, profile, writes};
    use ferrofed_identity::dev::Profile;

    #[test]
    fn a_run_is_refused_unless_writes_are_allowed() {
        assert!(matches!(writes(false), Err(Refusal::WritesNotAllowed)));
        assert!(writes(true).is_ok());
    }

    #[test]
    fn a_production_profile_needs_the_acknowledgement() {
        assert!(matches!(
            profile(Profile::Production, false),
            Err(Refusal::NotDevelopment)
        ));
        assert!(profile(Profile::Production, true).is_ok());
        assert!(profile(Profile::Development, false).is_ok());
    }
}
