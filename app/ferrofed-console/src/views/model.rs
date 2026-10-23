// SPDX-FileCopyrightText: Cadasto B.V.
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

/// The most rows a paged view asks the gateway for at once.
pub const PAGE_SIZE: u64 = 100;

/// What a server function of the console answers when the console itself
/// works: the view, or the expected refusal that kept it from showing.
///
/// A refusal is an answer, not an error: a server function that fails answers
/// with a `5xx`, which the browser logs as an error, and a caller branches on
/// the two, so they are distinct types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome<T> {
    /// The view.
    Shown(T),
    /// The gateway refused the request, or the operator's input cannot be
    /// sent.
    Refused(Refusal),
}

/// An expected refusal: the gateway's, with its status and stable code, or
/// the console's of input it cannot send.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Refusal {
    /// The gateway refused the request, with its status and its stable
    /// error code.
    Gateway {
        /// The status the gateway answered with.
        status: u16,
        /// The stable error code of its body, when it carried one.
        code: Option<String>,
    },
    /// The gateway did not accept the operator's access token (`401`), so
    /// the operator signs in again.
    NotAuthenticated {
        /// The stable error code of its body, when it carried one.
        code: Option<String>,
    },
    /// The operator's input cannot be sent as it stands. The reason names
    /// the field and never quotes what was entered (N33).
    Invalid {
        /// What is wrong, by field.
        reason: String,
    },
}

/// Why the console could not serve a view: a fault of its own, of its
/// session, or of its reach to the gateway, never an expected refusal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum ViewError {
    /// The request carries no live signed-in session.
    #[error("sign in to see this view")]
    SignedOut,
    /// The gateway answered with a body this console cannot read, such as a
    /// report of a shape it does not know.
    #[error("the gateway answered {status} with a body this console cannot read")]
    Unreadable {
        /// The status the gateway answered with.
        status: u16,
    },
    /// The gateway gave no answer at all.
    #[error("the gateway could not be reached")]
    Unreachable,
    /// A plain form post, which a page without its bundle sends, asked the
    /// query server function for an answer no page would show.
    #[error(
        "the query console runs a query once its page has loaded; reload the page and run it again"
    )]
    PlainPost,
    /// The console could not serve the view.
    #[error("the console could not serve this view")]
    Unavailable,
    /// The view could not be fetched from the console's server.
    #[error("the view could not be fetched: {source}")]
    Fetch {
        /// What the server function transport reported.
        #[source]
        source: ServerFnErrorErr,
    },
}

impl FromServerFnError for ViewError {
    type Encoder = JsonEncoding;

    fn from_server_fn_error(value: ServerFnErrorErr) -> Self {
        Self::Fetch { source: value }
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
    /// The path of the member's own page, keyed by its `node_id`, or by the
    /// `endpoint_id` of an endpoint that names no node, when that id is one
    /// a path carries as it stands.
    pub detail: Option<String>,
}

/// The members view: every member endpoint, and the other services the
/// gateway depends on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembersView {
    /// Every member endpoint, in the order `OPTIONS {base}/` lists them.
    pub members: Vec<MemberRow>,
    /// Every other dependency and its state, by key.
    pub services: Vec<(String, String)>,
    /// Which replica observed the health, as the view says it: each
    /// replica observes its own. `None` when the replica has no name.
    pub answered_by: Option<String>,
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
    /// Whether the operator cleared the `ehr_id` it is about after it was
    /// emitted.
    pub resolved: bool,
}

/// One collision the operator cleared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClearedRow {
    /// When it was cleared.
    pub at: String,
    /// The `ehr_id` cleared, only when it is a bare UUID.
    pub ehr_id: Option<String>,
    /// The members the gateway held it at, by `node_id`.
    pub claimants: Vec<String>,
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

/// The integrity view: the incidents of one state, the counts and the
/// collisions cleared, rendered on the server, and the collision clear.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrityView {
    /// The read-only part, rendered as HTML.
    pub html: String,
    /// The collision clear's fields, rendered on the server; `None` when
    /// the console is configured with no admin listener.
    pub clear_form: Option<String>,
}

/// The routing view: one page of the `creating_system_id` routing table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingView {
    /// The rows of the page.
    pub routes: Vec<RouteRow>,
    /// Where that page starts in the table.
    pub offset: u64,
    /// How many rows the whole table holds.
    pub total: u64,
    /// Which replica answered the table, as the view says it; `None` when
    /// the replica has no name.
    pub answered_by: Option<String>,
}

/// What the top bar says of the gateway that answered the console.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayView {
    /// The specification version the gateway implements, `major.minor`.
    pub spec_version: String,
    /// The federation's identifier.
    pub federation: String,
}

/// How the gateway that answered the console is deployed, as the top bar
/// and the banner under it say it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentView {
    /// Whether the gateway runs the development profile.
    pub development: bool,
    /// Whether patients resolve through the static development
    /// cross-reference.
    pub development_crossref: bool,
    /// The gateway's product version.
    pub version: String,
    /// The replica that answered; `None` when it has no name.
    pub replica: Option<String>,
}
