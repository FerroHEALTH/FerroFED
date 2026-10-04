// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `meta.federation`: everything the specification adds to an ITS-REST
//! `RESULT_SET`, under one unprefixed member (§9.1, N17, CP-35).

use std::collections::BTreeSet;

use serde::de::Deserializer;
use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Serialize};

use crate::error::WireError;
use crate::id::EndpointId;
use crate::object::{Extra, Members};
use crate::outcome::{EndpointOutcome, optional_entry};
use crate::status::EndpointStatus;

/// The effective timeout budget of one request, `meta.federation.timeout`
/// (§11.5, N38).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TimeoutBudget {
    /// The budget for one node's request, in milliseconds.
    pub per_node_ms: Option<u64>,
    /// The budget for the whole fan-out, in milliseconds.
    pub overall_ms: Option<u64>,
    /// The completion policy the budget applies under.
    pub policy: Option<String>,
    /// The members this type does not model.
    pub extra: Extra,
}

plain_record!(TimeoutBudget, "timeout", {
    per_node_ms: optional,
    overall_ms: optional,
    policy: optional,
});

/// The deduplication applied to one result set, `meta.federation.dedup`
/// (§10, N15, N36).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DedupRecord {
    /// The mode applied; `none` is the default.
    pub mode: Option<String>,
    /// The rows the mode suppressed.
    pub suppressed_rows: Option<u64>,
    /// The endpoints whose copies were dropped (§10.3, N36), so a client can
    /// tell that a write it issues will not update them.
    pub suppressed_endpoints: Option<Vec<EndpointId>>,
    /// The members this type does not model.
    pub extra: Extra,
}

plain_record!(DedupRecord, "dedup", {
    mode: optional,
    suppressed_rows: optional,
    suppressed_endpoints: optional,
});

/// The `meta.federation` object (§9.1, §9.5, §11.4).
///
/// `complete` is never set by hand: it is true exactly when every in-scope
/// endpoint reached `active` (§11.4, N37), so it is derived from the
/// outcomes, and a reader refuses an envelope whose declared value disagrees.
/// An endpoint reported `excluded` or `not-localized` was never in scope and
/// does not clear it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FederationMeta {
    endpoints: Vec<EndpointOutcome>,
    timeout: Option<TimeoutBudget>,
    dedup: Option<DedupRecord>,
    extra: Extra,
}

impl FederationMeta {
    /// The member names this type models, as the schema spells them.
    pub const MEMBERS: &[&str] = &["complete", "endpoints", "timeout", "dedup"];

    /// The federation record of a query whose endpoints ended as `endpoints`.
    ///
    /// # Errors
    ///
    /// Returns [`WireError::DuplicateEndpoint`] when two outcomes name the
    /// same endpoint (N16: every in-scope node appears with a status).
    pub fn new(endpoints: Vec<EndpointOutcome>) -> Result<Self, WireError> {
        let mut seen = BTreeSet::new();
        for outcome in &endpoints {
            if !seen.insert(outcome.id()) {
                return Err(WireError::DuplicateEndpoint {
                    id: outcome.id().as_str().to_owned(),
                });
            }
        }
        Ok(Self {
            endpoints,
            timeout: None,
            dedup: None,
            extra: Extra::new(),
        })
    }

    /// Sets the effective timeout budget, which the envelope SHOULD carry
    /// (§11.5).
    #[must_use]
    pub fn with_timeout(mut self, timeout: TimeoutBudget) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Sets the deduplication record.
    #[must_use]
    pub fn with_dedup(mut self, dedup: DedupRecord) -> Self {
        self.dedup = Some(dedup);
        self
    }

    /// Whether every in-scope endpoint reached `active` (§11.4, N37).
    #[must_use]
    pub fn complete(&self) -> bool {
        self.endpoints
            .iter()
            .map(EndpointOutcome::status)
            .filter(|status| status.is_in_scope())
            .all(|status| status == EndpointStatus::Active)
    }

    /// The per-endpoint record, in the order the gateway reported it.
    #[must_use]
    pub fn endpoints(&self) -> &[EndpointOutcome] {
        &self.endpoints
    }

    /// The effective timeout budget, if reported.
    #[must_use]
    pub fn timeout(&self) -> Option<&TimeoutBudget> {
        self.timeout.as_ref()
    }

    /// The deduplication record, if reported.
    #[must_use]
    pub fn dedup(&self) -> Option<&DedupRecord> {
        self.dedup.as_ref()
    }

    /// The members this type does not model.
    #[must_use]
    pub fn extra(&self) -> &Extra {
        &self.extra
    }

    /// The members this type does not model, for the federation-level
    /// diagnostics a deployment adds (§11.6.4, §14.1).
    pub fn extra_mut(&mut self) -> &mut Extra {
        &mut self.extra
    }
}

impl Serialize for FederationMeta {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("complete", &self.complete())?;
        map.serialize_entry("endpoints", &self.endpoints)?;
        optional_entry(&mut map, "timeout", self.timeout.as_ref())?;
        optional_entry(&mut map, "dedup", self.dedup.as_ref())?;
        self.extra.write(&mut map, "federation", Self::MEMBERS)?;
        map.end()
    }
}

impl<'de> Deserialize<'de> for FederationMeta {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut members = Members::read("federation", deserializer)?;
        read_meta(&mut members)
            .map(|mut meta| {
                meta.extra = members.into_extra();
                meta
            })
            .map_err(serde::de::Error::custom)
    }
}

fn read_meta(members: &mut Members) -> Result<FederationMeta, WireError> {
    let declared: bool = members.required("complete")?;
    let mut meta = FederationMeta::new(members.required("endpoints")?)?;
    meta.timeout = members.optional("timeout")?;
    meta.dedup = members.optional("dedup")?;
    let derived = meta.complete();
    if declared != derived {
        return Err(WireError::CompleteMismatch { declared, derived });
    }
    Ok(meta)
}
