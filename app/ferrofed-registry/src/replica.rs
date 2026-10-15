// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The name of one gateway process, which every report of per-process state
//! carries.
//!
//! The integrity incidents, the learned routing table and the dependency
//! states live in each process, so a deployment of several replicas holds
//! one of each per replica. Every report of the read-only operator surface
//! and the export of the identity state name the replica that produced
//! them, so a reader never takes one replica's view for the federation's.
//! A name is a short token the operator chooses or the platform gives the
//! process, never a patient identifier, and it never reaches a node.
//!
//! ```
//! use ferrofed_registry::replica::Replica;
//!
//! let replica: Replica = "ferrofed-7d9c4b5f6-x2k8q".parse()?;
//! assert_eq!("ferrofed-7d9c4b5f6-x2k8q", replica.as_str());
//! assert!("two words".parse::<Replica>().is_err());
//! assert_eq!("unnamed", Replica::default().as_str());
//! # Ok::<(), ferrofed_registry::replica::ReplicaError>(())
//! ```
// NOTE: no specification governs this: our own design; the form is one a
// Kubernetes pod name and a host name both take.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// The longest replica name, in bytes.
pub const MAX_LEN: usize = 64;

/// The name of a replica that was given none and runs where the platform
/// names no host.
pub const UNNAMED: &str = "unnamed";

/// The response header every answer of the operator surface names its
/// replica in.
pub const HEADER: &str = "ferrofed-replica";

/// The name of one gateway process: 1 to [`MAX_LEN`] ASCII letters, digits,
/// `.`, `_` and `-`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Replica(String);

impl Replica {
    /// Returns the name as it is written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for Replica {
    fn default() -> Self {
        Self(UNNAMED.to_owned())
    }
}

impl fmt::Display for Replica {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Replica {
    type Err = ReplicaError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text.is_empty() {
            return Err(ReplicaError::Empty);
        }
        if text.len() > MAX_LEN {
            return Err(ReplicaError::TooLong { length: text.len() });
        }
        if let Some(found) = text
            .chars()
            .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')))
        {
            return Err(ReplicaError::Character { found });
        }
        Ok(Self(text.to_owned()))
    }
}

impl TryFrom<String> for Replica {
    type Error = ReplicaError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<Replica> for String {
    fn from(replica: Replica) -> Self {
        replica.0
    }
}

/// Why a text is no replica name.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReplicaError {
    /// The name is empty.
    #[error("a replica name is never empty")]
    Empty,
    /// The name is longer than [`MAX_LEN`] bytes.
    #[error("a replica name is at most {MAX_LEN} bytes, and this one is {length}")]
    TooLong {
        /// How long the name is, in bytes.
        length: usize,
    },
    /// The name holds a character the form does not admit.
    #[error("a replica name holds ASCII letters, digits, '.', '_' and '-' only, and not {found:?}")]
    Character {
        /// The first character the form does not admit.
        found: char,
    },
}

#[cfg(test)]
mod tests {
    use super::{MAX_LEN, Replica, ReplicaError, UNNAMED};

    #[test]
    fn a_pod_name_and_a_host_name_are_replica_names() {
        for name in ["ferrofed-0", "gw_a.example.org", "A", &"a".repeat(MAX_LEN)] {
            assert_eq!(
                Ok(name),
                name.parse::<Replica>().as_ref().map(Replica::as_str)
            );
        }
    }

    #[test]
    fn an_empty_long_or_spaced_name_is_refused() {
        assert_eq!(Err(ReplicaError::Empty), "".parse::<Replica>());
        assert_eq!(
            Err(ReplicaError::TooLong {
                length: MAX_LEN + 1
            }),
            "a".repeat(MAX_LEN + 1).parse::<Replica>()
        );
        assert_eq!(
            Err(ReplicaError::Character { found: ' ' }),
            "pod a".parse::<Replica>()
        );
        assert_eq!(
            Err(ReplicaError::Character { found: '/' }),
            "pod/a".parse::<Replica>()
        );
    }

    #[test]
    fn a_name_reads_back_as_the_string_it_is() {
        let replica: Replica = serde_json::from_str("\"pod-a\"").expect("a replica");
        assert_eq!(
            "\"pod-a\"",
            serde_json::to_string(&replica).expect("written")
        );
        assert!(serde_json::from_str::<Replica>("\"pod a\"").is_err());
        assert_eq!(UNNAMED, Replica::default().as_str());
    }
}
