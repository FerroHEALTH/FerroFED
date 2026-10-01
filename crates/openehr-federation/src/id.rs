// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The registry identifiers the wire carries, as distinct types.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::WireError;

/// The `endpoint_id`: the stable registry identifier of one endpoint (§ The
/// four identifiers, N19).
///
/// It is the `id` of a `meta.federation.endpoints[]` entry and of an
/// `OPTIONS` member endpoint, and the value a `FROM ENDPOINT` directive or the
/// `openEHR-federation-endpoint` header names. The schemas require it to be
/// non-empty.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EndpointId(String);

impl EndpointId {
    /// Checks that `id` is a non-empty endpoint identifier.
    ///
    /// # Errors
    ///
    /// Returns [`WireError::EmptyMember`] when `id` is empty.
    pub fn new(id: impl Into<String>) -> Result<Self, WireError> {
        let id = id.into();
        if id.is_empty() {
            return Err(WireError::EmptyMember {
                object: "endpoint",
                member: "id",
            });
        }
        Ok(Self(id))
    }

    /// The identifier as the registry spells it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EndpointId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for EndpointId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for EndpointId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// The federation's own identifier, `federation.id` of the `OPTIONS` body
/// (N30), required to be non-empty.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FederationId(String);

impl FederationId {
    /// Checks that `id` is a non-empty federation identifier.
    ///
    /// # Errors
    ///
    /// Returns [`WireError::EmptyMember`] when `id` is empty.
    pub fn new(id: impl Into<String>) -> Result<Self, WireError> {
        let id = id.into();
        if id.is_empty() {
            return Err(WireError::EmptyMember {
                object: "federation",
                member: "id",
            });
        }
        Ok(Self(id))
    }

    /// The identifier as the federation spells it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FederationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for FederationId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for FederationId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}
