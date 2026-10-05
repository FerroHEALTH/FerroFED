// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What each operator view renders, as the server sends it to the browser,
//! and why a view could not be rendered.
//!
//! The models hold routing ids, states, counts and stored definitions only,
//! never a token and never a patient identifier, because each is serialized
//! into the page the browser hydrates (§5.4.1, N33). Every integer is of a
//! fixed size, because the browser half is 32-bit.

use leptos::server_fn::codec::JsonEncoding;
use leptos::server_fn::error::{FromServerFnError, ServerFnErrorErr};
use serde::{Deserialize, Serialize};

/// Why a view could not be rendered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum ViewError {
    /// The request carries no live signed-in session.
    #[error("sign in to see this view")]
    SignedOut,
    /// The gateway refused the request, with its status and its stable
    /// error code.
    #[error("the gateway answered {status}")]
    Refused {
        /// The status the gateway answered with.
        status: u16,
        /// The stable error code of its body, when it carried one.
        code: Option<String>,
    },
    /// The gateway gave no answer the console can use.
    #[error("the gateway could not be reached")]
    Unreachable,
    /// The console could not serve the view.
    #[error("the console could not serve this view")]
    Unavailable,
    /// The view could not be fetched from the console's server.
    #[error("the view could not be fetched: {reason}")]
    Fetch {
        /// What the server function transport reported.
        reason: String,
    },
}

impl FromServerFnError for ViewError {
    type Encoder = JsonEncoding;

    fn from_server_fn_error(value: ServerFnErrorErr) -> Self {
        Self::Fetch {
            reason: value.to_string(),
        }
    }
}

/// One member endpoint and its health.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberRow {
    /// The `endpoint_id`.
    pub endpoint_id: String,
    /// The managing organisation.
    pub organisation: String,
    /// The membership standing, as `OPTIONS {base}/` declares it.
    pub status: String,
    /// The owning node.
    pub node_id: Option<String>,
    /// The node's `system_id`.
    pub system_id: Option<String>,
    /// The product and its version, when the registry names them.
    pub product: Option<String>,
    /// The median latency over recent requests, in milliseconds.
    pub latency_ms_p50: Option<u64>,
    /// The last state the gateway observed of the endpoint.
    pub health: String,
}

/// The members view: every member endpoint, and the other services the
/// gateway depends on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembersView {
    /// Every member endpoint, in the order `OPTIONS {base}/` lists them.
    pub members: Vec<MemberRow>,
    /// Every other dependency and its state, by key.
    pub services: Vec<(String, String)>,
}

/// One integrity incident.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncidentRow {
    /// When it was emitted.
    pub at: String,
    /// Its kind.
    pub kind: String,
    /// Its description, which names an `ehr_id` only when it is a bare UUID.
    pub description: String,
}

/// One row of the `creating_system_id` routing table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteRow {
    /// The `creating_system_id`.
    pub creating_system_id: String,
    /// Where the route comes from: `member`, `registered`, `learned` or
    /// `withdrawn`.
    pub source: String,
    /// The node it routes to.
    pub node: Option<String>,
    /// The endpoint it routes through.
    pub endpoint: Option<String>,
}

/// The integrity view: the incident counts, the recent incidents, and the
/// routing table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrityView {
    /// How many incidents of each kind were emitted since the gateway
    /// started.
    pub counts: Vec<(String, u64)>,
    /// The most recent incidents, newest first.
    pub recent: Vec<IncidentRow>,
    /// The `creating_system_id` routing table.
    pub routes: Vec<RouteRow>,
}

/// One held stored-query version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredRow {
    /// The qualified name.
    pub name: String,
    /// The version.
    pub version: String,
    /// When it was stored.
    pub saved: String,
    /// The AQL text.
    pub aql: String,
}

/// The stored-query view.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredView {
    /// Every held version, by name and then by version.
    pub definitions: Vec<StoredRow>,
}

/// The self-description view, `OPTIONS {base}/`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FederationView {
    /// The federation's identifier.
    pub id: String,
    /// The specification version the gateway implements.
    pub spec_version: String,
    /// How many member endpoints it declares.
    pub endpoints: u32,
    /// The `federation` object as the gateway declares it, pretty-printed.
    pub document: String,
}
