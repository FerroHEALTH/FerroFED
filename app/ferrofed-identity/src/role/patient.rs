// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient identifier inside the gateway: consumed by resolution, never
//! dispatched, logged, traced, measured or serialized (§5.4, N33).

use std::fmt;

use ferrofed_registry::secret::REDACTED;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use thiserror::Error;

/// A [`PatientRef`] or [`IdentifierNamespace`] that cannot be built.
///
/// The errors name what is wrong, never the identifier's value.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum PatientRefError {
    /// The issuing namespace is empty.
    #[error("an empty identifier namespace")]
    EmptyNamespace,
    /// The identifier value is empty.
    #[error("an empty patient identifier")]
    EmptyValue,
}

/// The namespace that issued a patient identifier: `PARTY_REF.namespace`, or
/// the `DV_IDENTIFIER.issuer` or `type` of an `ENTRY`-level subject (§5.4.3).
///
/// A namespace names an identifier space, never a patient, so it is ordinary
/// data: it may appear in a log or an error.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(try_from = "String")]
pub struct IdentifierNamespace(String);

impl IdentifierNamespace {
    /// Builds a namespace from its string form.
    ///
    /// # Errors
    ///
    /// [`PatientRefError::EmptyNamespace`] for an empty value.
    pub fn new(value: impl Into<String>) -> Result<Self, PatientRefError> {
        let value = value.into();
        if value.is_empty() {
            return Err(PatientRefError::EmptyNamespace);
        }
        Ok(Self(value))
    }

    /// The namespace as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for IdentifierNamespace {
    type Error = PatientRefError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl fmt::Display for IdentifierNamespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A patient identifier the client presented, with the namespace that issued
/// it.
///
/// The gateway consumes it in resolution, and it reaches no node, log, trace,
/// metric or store (§5.4.1, N33). A pseudonym is held exactly like a direct
/// identifier (§5.3, §B.7). The value is a [`SecretString`], `Debug` and
/// `Display` are redacted, and the type is never `Serialize`:
///
/// ```
/// use ferrofed_identity::role::patient::{IdentifierNamespace, PatientRef};
///
/// let patient = PatientRef::new(IdentifierNamespace::new("2.999.1")?, "12345".into())?;
/// assert!(!format!("{patient} {patient:?}").contains("12345"));
/// # Ok::<(), ferrofed_identity::role::patient::PatientRefError>(())
/// ```
pub struct PatientRef {
    namespace: IdentifierNamespace,
    value: SecretString,
}

impl PatientRef {
    /// Builds a patient reference.
    ///
    /// # Errors
    ///
    /// [`PatientRefError::EmptyValue`] for an empty identifier.
    pub fn new(
        namespace: IdentifierNamespace,
        value: SecretString,
    ) -> Result<Self, PatientRefError> {
        if value.expose_secret().is_empty() {
            return Err(PatientRefError::EmptyValue);
        }
        Ok(Self { namespace, value })
    }

    /// The namespace that issued the identifier.
    #[must_use]
    pub fn namespace(&self) -> &IdentifierNamespace {
        &self.namespace
    }

    /// The identifier value, for a cross-reference lookup inside this crate.
    pub(crate) fn value(&self) -> &str {
        self.value.expose_secret()
    }

    /// The identifier, still a [`SecretString`], for the outbound gate to
    /// withhold from every request to a node (§5.4.1, N33): a master identity
    /// the demographics step found is as identifying as the client's own.
    #[must_use]
    pub fn withheld(&self) -> SecretString {
        self.value.clone()
    }
}

impl fmt::Debug for PatientRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PatientRef")
            .field("namespace", &self.namespace)
            .field("value", &REDACTED)
            .finish()
    }
}

impl fmt::Display for PatientRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a patient identifier in namespace {}", self.namespace)
    }
}
