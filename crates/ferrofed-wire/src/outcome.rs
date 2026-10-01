// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One `meta.federation.endpoints[]` entry: what happened to one endpoint
//! during one query (§9.5, §11.1, N16, N40).
//!
//! The obligations the schema states as `if`/`then` conditionals are the shape
//! of [`Outcome`]: a status that requires `error` or `latency_ms` has a variant
//! that carries it, and a status settled before any request existed has no
//! `latency_ms` to set, so a gateway cannot emit the `0` that §9.5 forbids.

use serde::de::Deserializer;
use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::error::WireError;
use crate::id::EndpointId;
use crate::object::{Extra, Members, Uri};
use crate::status::EndpointStatus;

/// The node-reported or gateway-reported error of an endpoint outcome
/// (§11.2), a string or an object as the schema allows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorDetail {
    /// A non-empty message.
    Text(String),
    /// A structured error, kept member by member as raw JSON.
    Object(Extra),
}

impl ErrorDetail {
    /// A message error.
    ///
    /// # Errors
    ///
    /// Returns [`WireError::EmptyMember`] when `message` is empty: an empty
    /// error carries nothing, and N40 requires the error itself.
    pub fn text(message: impl Into<String>) -> Result<Self, WireError> {
        let message = message.into();
        if message.is_empty() {
            return Err(WireError::EmptyMember {
                object: "endpoint",
                member: "error",
            });
        }
        Ok(Self::Text(message))
    }
}

impl Serialize for ErrorDetail {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Text(message) => serializer.serialize_str(message),
            Self::Object(members) => {
                let mut map = serializer.serialize_map(Some(members.len()))?;
                members.write(&mut map, "error", &[])?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for ErrorDetail {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(deserializer)?;
        let text = raw.get().trim_start();
        if text.starts_with('"') {
            let message: String = serde_json::from_str(text).map_err(serde::de::Error::custom)?;
            return Self::text(message).map_err(serde::de::Error::custom);
        }
        if text.starts_with('{') {
            let mut object = serde_json::Deserializer::from_str(text);
            let members = Members::read("error", &mut object).map_err(serde::de::Error::custom)?;
            return Ok(Self::Object(members.into_extra()));
        }
        Err(serde::de::Error::custom(
            "`error` must be a string or an object (§9.5)",
        ))
    }
}

/// Who refused an endpoint on consent grounds (§11.1, §13.2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentRefusal {
    /// A Step-1 consent pre-filter (N27a): no request was dispatched, so
    /// there is no latency to report.
    PreFilter,
    /// The node itself refused (N27) after the gateway dispatched to it.
    Node {
        /// The gateway's own wall-clock measurement of the request, in
        /// milliseconds.
        latency_ms: u64,
    },
}

/// The status of one endpoint together with the members that status
/// requires or allows (§9.5, §11.1, N40).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Outcome {
    /// Queried and responded.
    Active {
        /// The gateway's measurement of the request, in milliseconds.
        latency_ms: u64,
    },
    /// Known, not reachable.
    Offline {
        /// The gateway's measurement of the attempt, in milliseconds.
        latency_ms: u64,
        /// What failed.
        error: ErrorDetail,
    },
    /// Reachable, no response within the timeout (§11.5).
    TimeOut {
        /// The elapsed time at abandonment, in milliseconds.
        latency_ms: u64,
        /// What failed.
        error: ErrorDetail,
    },
    /// Reached, answered with a failure; `error` carries the node's own
    /// failure, at minimum its HTTP status.
    NodeError {
        /// The gateway's measurement of the request, in milliseconds.
        latency_ms: u64,
        /// The node's failure.
        error: ErrorDetail,
    },
    /// No local `ehr_id` for the patient at this node (§5, N6), settled in
    /// Step-1 resolution before any request existed.
    NotResolved {
        /// What the cross-reference service reported.
        error: ErrorDetail,
    },
    /// Consent did not permit inclusion.
    ConsentDenied {
        /// The pre-filter or the node.
        refused_by: ConsentRefusal,
        /// The refusal, when one was reported.
        error: Option<ErrorDetail>,
    },
    /// Ruled out by a decision about this node; never in scope.
    Excluded {
        /// A reported reason, if any.
        error: Option<ErrorDetail>,
    },
    /// Not returned by localization; never in scope (§14). `error` carries
    /// the localization error when localization was unavailable (§14.1).
    NotLocalized {
        /// The localization error, if any.
        error: Option<ErrorDetail>,
    },
}

impl Outcome {
    /// The §11.1 status this outcome reports.
    #[must_use]
    pub const fn status(&self) -> EndpointStatus {
        match self {
            Self::Active { .. } => EndpointStatus::Active,
            Self::Offline { .. } => EndpointStatus::Offline,
            Self::TimeOut { .. } => EndpointStatus::TimeOut,
            Self::NodeError { .. } => EndpointStatus::NodeError,
            Self::NotResolved { .. } => EndpointStatus::NotResolved,
            Self::ConsentDenied { .. } => EndpointStatus::ConsentDenied,
            Self::Excluded { .. } => EndpointStatus::Excluded,
            Self::NotLocalized { .. } => EndpointStatus::NotLocalized,
        }
    }

    /// The gateway's measurement of the request, present exactly when a
    /// request was dispatched (N40).
    #[must_use]
    pub const fn latency_ms(&self) -> Option<u64> {
        match self {
            Self::Active { latency_ms }
            | Self::Offline { latency_ms, .. }
            | Self::TimeOut { latency_ms, .. }
            | Self::NodeError { latency_ms, .. }
            | Self::ConsentDenied {
                refused_by: ConsentRefusal::Node { latency_ms },
                ..
            } => Some(*latency_ms),
            Self::NotResolved { .. }
            | Self::ConsentDenied {
                refused_by: ConsentRefusal::PreFilter,
                ..
            }
            | Self::Excluded { .. }
            | Self::NotLocalized { .. } => None,
        }
    }

    /// The reported error, if the outcome carries one.
    #[must_use]
    pub const fn error(&self) -> Option<&ErrorDetail> {
        match self {
            Self::Offline { error, .. }
            | Self::TimeOut { error, .. }
            | Self::NodeError { error, .. }
            | Self::NotResolved { error } => Some(error),
            Self::ConsentDenied { error, .. }
            | Self::Excluded { error }
            | Self::NotLocalized { error } => error.as_ref(),
            Self::Active { .. } => None,
        }
    }

    /// Builds the outcome a reader found, refusing the combinations §9.5,
    /// §11.1 and N40 rule out.
    fn from_members(
        status: EndpointStatus,
        latency_ms: Option<u64>,
        error: Option<ErrorDetail>,
    ) -> Result<Self, WireError> {
        let requires = |member| WireError::StatusRequires { status, member };
        let forbids = |member| WireError::StatusForbids { status, member };
        let dispatched = |latency_ms: Option<u64>| latency_ms.ok_or_else(|| requires("latency_ms"));
        let failed = |error: Option<ErrorDetail>| error.ok_or_else(|| requires("error"));
        match status {
            EndpointStatus::Active => match error {
                // NOTE: §11.1, a node error is never reported as `active` with an error attached.
                Some(_) => Err(forbids("error")),
                None => Ok(Self::Active {
                    latency_ms: dispatched(latency_ms)?,
                }),
            },
            EndpointStatus::Offline => Ok(Self::Offline {
                latency_ms: dispatched(latency_ms)?,
                error: failed(error)?,
            }),
            EndpointStatus::TimeOut => Ok(Self::TimeOut {
                latency_ms: dispatched(latency_ms)?,
                error: failed(error)?,
            }),
            EndpointStatus::NodeError => Ok(Self::NodeError {
                latency_ms: dispatched(latency_ms)?,
                error: failed(error)?,
            }),
            EndpointStatus::NotResolved => match latency_ms {
                Some(_) => Err(forbids("latency_ms")),
                None => Ok(Self::NotResolved {
                    error: failed(error)?,
                }),
            },
            EndpointStatus::ConsentDenied => Ok(Self::ConsentDenied {
                refused_by: latency_ms.map_or(ConsentRefusal::PreFilter, |latency_ms| {
                    ConsentRefusal::Node { latency_ms }
                }),
                error,
            }),
            EndpointStatus::Excluded => match latency_ms {
                Some(_) => Err(forbids("latency_ms")),
                None => Ok(Self::Excluded { error }),
            },
            EndpointStatus::NotLocalized => match latency_ms {
                Some(_) => Err(forbids("latency_ms")),
                None => Ok(Self::NotLocalized { error }),
            },
        }
    }
}

/// One `meta.federation.endpoints[]` entry (§9.5).
///
/// `id` and the [`Outcome`] are required. The SHOULD members of the §9.5
/// table are optional here, and a gateway that does not know `product` or
/// `version` leaves them unset rather than inventing them (N40).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointOutcome {
    id: EndpointId,
    outcome: Outcome,
    node_id: Option<String>,
    system_id: Option<String>,
    organisation: Option<String>,
    product: Option<String>,
    version: Option<String>,
    row_count: Option<u64>,
    url: Option<Uri>,
    extra: Extra,
}

impl EndpointOutcome {
    /// The member names this type models, as the schema spells them.
    pub const MEMBERS: &'static [&'static str] = &[
        "id",
        "status",
        "latency_ms",
        "node_id",
        "system_id",
        "organisation",
        "error",
        "product",
        "version",
        "row_count",
        "url",
    ];

    /// An outcome for endpoint `id` with no optional member set.
    #[must_use]
    pub fn new(id: EndpointId, outcome: Outcome) -> Self {
        Self {
            id,
            outcome,
            node_id: None,
            system_id: None,
            organisation: None,
            product: None,
            version: None,
            row_count: None,
            url: None,
            extra: Extra::new(),
        }
    }

    /// Sets the owning node (`node_id`).
    #[must_use]
    pub fn with_node_id(mut self, node_id: impl Into<String>) -> Self {
        self.node_id = Some(node_id.into());
        self
    }

    /// Sets the node's openEHR `system_id`.
    #[must_use]
    pub fn with_system_id(mut self, system_id: impl Into<String>) -> Self {
        self.system_id = Some(system_id.into());
        self
    }

    /// Sets the managing Organization (N20).
    #[must_use]
    pub fn with_organisation(mut self, organisation: impl Into<String>) -> Self {
        self.organisation = Some(organisation.into());
        self
    }

    /// Sets the node's product name, as the gateway knows it.
    #[must_use]
    pub fn with_product(mut self, product: impl Into<String>) -> Self {
        self.product = Some(product.into());
        self
    }

    /// Sets the node's product version, as the gateway knows it.
    #[must_use]
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Sets the CDR base URL.
    #[must_use]
    pub fn with_url(mut self, url: Uri) -> Self {
        self.url = Some(url);
        self
    }

    /// Sets the rows this endpoint contributed before federation-level
    /// `DISTINCT`, dedup and `LIMIT`.
    ///
    /// # Errors
    ///
    /// Returns [`WireError::RowsWithoutAnswer`] when `row_count` is above 0
    /// and the endpoint did not reach `active`: an unresponsive node
    /// contributes no rows (§11.1).
    pub fn with_row_count(mut self, row_count: u64) -> Result<Self, WireError> {
        check_rows(self.outcome.status(), Some(row_count))?;
        self.row_count = Some(row_count);
        Ok(self)
    }

    /// The endpoint that was addressed.
    #[must_use]
    pub fn id(&self) -> &EndpointId {
        &self.id
    }

    /// The status and the members it carries.
    #[must_use]
    pub fn outcome(&self) -> &Outcome {
        &self.outcome
    }

    /// The §11.1 status.
    #[must_use]
    pub fn status(&self) -> EndpointStatus {
        self.outcome.status()
    }

    /// The owning node, if reported.
    #[must_use]
    pub fn node_id(&self) -> Option<&str> {
        self.node_id.as_deref()
    }

    /// The node's openEHR `system_id`, if reported.
    #[must_use]
    pub fn system_id(&self) -> Option<&str> {
        self.system_id.as_deref()
    }

    /// The managing Organization, if reported.
    #[must_use]
    pub fn organisation(&self) -> Option<&str> {
        self.organisation.as_deref()
    }

    /// The node's product name, if known.
    #[must_use]
    pub fn product(&self) -> Option<&str> {
        self.product.as_deref()
    }

    /// The node's product version, if known.
    #[must_use]
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    /// The rows contributed before federation-level suppression, if
    /// reported.
    #[must_use]
    pub fn row_count(&self) -> Option<u64> {
        self.row_count
    }

    /// The CDR base URL, if reported.
    #[must_use]
    pub fn url(&self) -> Option<&Uri> {
        self.url.as_ref()
    }

    /// The members this type does not model.
    #[must_use]
    pub fn extra(&self) -> &Extra {
        &self.extra
    }

    /// The members this type does not model, for a deployment's own
    /// diagnostics.
    pub fn extra_mut(&mut self) -> &mut Extra {
        &mut self.extra
    }
}

fn check_rows(status: EndpointStatus, row_count: Option<u64>) -> Result<(), WireError> {
    match row_count {
        Some(rows) if rows > 0 && status != EndpointStatus::Active => {
            Err(WireError::RowsWithoutAnswer { status })
        }
        _ => Ok(()),
    }
}

impl Serialize for EndpointOutcome {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("id", &self.id)?;
        map.serialize_entry("status", &self.outcome.status())?;
        if let Some(latency_ms) = self.outcome.latency_ms() {
            map.serialize_entry("latency_ms", &latency_ms)?;
        }
        optional_entry(&mut map, "node_id", self.node_id.as_ref())?;
        optional_entry(&mut map, "system_id", self.system_id.as_ref())?;
        optional_entry(&mut map, "organisation", self.organisation.as_ref())?;
        optional_entry(&mut map, "error", self.outcome.error())?;
        optional_entry(&mut map, "product", self.product.as_ref())?;
        optional_entry(&mut map, "version", self.version.as_ref())?;
        optional_entry(&mut map, "row_count", self.row_count.as_ref())?;
        optional_entry(&mut map, "url", self.url.as_ref())?;
        self.extra.write(&mut map, "endpoint", Self::MEMBERS)?;
        map.end()
    }
}

impl<'de> Deserialize<'de> for EndpointOutcome {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut members = Members::read("endpoint", deserializer)?;
        read_outcome(&mut members)
            .map(|mut outcome| {
                outcome.extra = members.into_extra();
                outcome
            })
            .map_err(serde::de::Error::custom)
    }
}

fn read_outcome(members: &mut Members) -> Result<EndpointOutcome, WireError> {
    let id = members.required("id")?;
    let status: EndpointStatus = members.required("status")?;
    let latency_ms = members.optional("latency_ms")?;
    let error = members.optional("error")?;
    let row_count = members.optional("row_count")?;
    check_rows(status, row_count)?;
    Ok(EndpointOutcome {
        id,
        outcome: Outcome::from_members(status, latency_ms, error)?,
        node_id: members.optional("node_id")?,
        system_id: members.optional("system_id")?,
        organisation: members.optional("organisation")?,
        product: members.optional("product")?,
        version: members.optional("version")?,
        row_count,
        url: members.optional("url")?,
        extra: Extra::new(),
    })
}

/// Writes `name` only when `value` is present.
pub(crate) fn optional_entry<M: SerializeMap, T: Serialize + ?Sized>(
    map: &mut M,
    name: &'static str,
    value: Option<&T>,
) -> Result<(), M::Error> {
    match value {
        Some(value) => map.serialize_entry(name, value),
        None => Ok(()),
    }
}
