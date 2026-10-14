// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The pass-through to a member's own exchange-format endpoint: one
//! `GET {fhir}/Patient/$summary` to the member's native endpoint, and the
//! member's answer returned as it sent it.
//!
//! The request is the International Patient Summary `$summary` operation,
//! naming the patient by `identifier=<system>|<ehr_id>`, the member's own
//! `ehr_id` in the identifier system the registry declares for it, and the
//! document's `profile`. The member is located by its `ehr_id` alone, the
//! one value about the patient the request carries (§5.4.1, N33). It leaves
//! through the member's Query API client
//! ([`NodeClient::client_at`](crate::dispatch::NodeClient)): over the same
//! transport, with the same onward credentials and `DPoP` prover, the
//! caller's identity signed for that endpoint, the minted `X-Request-Id`, and
//! a slot of its in-flight cap, and it passes the same outbound gate, which
//! reads its path, its query and the caller claims it conveys against the
//! identifiers resolution consumed. The answer is a [`Forwarded`] whose body
//! is the member's bytes, unchanged.
//!
//! The answer is read as the probe reads one: a success is the member's
//! document when it is a FHIR document `Bundle` whose identifier has a
//! system and a value (FHIR R4 `bdl-9`), `404` says it holds none for that
//! `ehr_id`, and any other success, every other status, or no answer at
//! all says nothing about the patient and is a failure the caller reports
//! with its §11.1 status, never an empty document (§11.2, §11.3).

use ferrofed_registry::id::EhrId;
use ferrofed_registry::native::NativeEndpoint;
use http::header::ACCEPT;
use http::{HeaderValue, Method, StatusCode};
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::client::{Request, Transport};

use crate::dispatch::{Contact, DispatchOptions, NodeClient, OptionsError};
use crate::hygiene::{Composed, Outbound};
use crate::single_node::forward::{ForwardError, Forwarded};
use crate::trace_context;

/// The path of the summary operation under the member's FHIR base.
pub const SUMMARY_PATH: &str = "/Patient/$summary";

/// The FHIR JSON media type the request accepts (FHIR R4
/// `http.html#mime-type`).
pub const FHIR_JSON: &str = "application/fhir+json";

/// The operation name a native call is traced under.
const OPERATION: &str = "native_summary";

/// What one request for a member's own patient summary asks.
#[derive(Debug, Clone, Copy)]
pub struct SummaryAsked<'a> {
    /// The member's native endpoint.
    pub native: &'a NativeEndpoint,
    /// The member's own `ehr_id`, the one value about the patient the
    /// request carries.
    pub ehr_id: &'a EhrId,
    /// The canonical URL of the profile the document follows.
    pub profile: &'a str,
}

/// A member's own document: its bytes as it sent them, and the
/// `Bundle.identifier` they carry (FHIR R4 `bdl-9`).
///
/// `Debug` shows the answer's status and sizes and the identifier's system,
/// never the body or the identifier's value.
pub struct Document {
    forwarded: Forwarded,
    system: String,
    value: String,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document")
            .field("forwarded", &self.forwarded)
            .field("system", &self.system)
            .finish_non_exhaustive()
    }
}

impl Document {
    /// Reads `forwarded` as a member's document.
    ///
    /// # Errors
    ///
    /// The [`Unusable`] that says why the body is no FHIR document `Bundle`
    /// whose identifier has a system and a value, which FHIR R4 `bdl-9`
    /// requires of every document.
    pub fn read(forwarded: Forwarded) -> Result<Self, Unusable> {
        let head: Head = serde_json::from_slice(forwarded.body()).map_err(Unusable::Unreadable)?;
        if head.resource_type.as_deref() != Some("Bundle") {
            return Err(Unusable::NotBundle);
        }
        if head.kind.as_deref() != Some("document") {
            return Err(Unusable::NotDocument);
        }
        let identifier = head.identifier.ok_or(Unusable::NoIdentifier)?;
        match (identifier.system, identifier.value) {
            (Some(system), Some(value)) if !system.is_empty() && !value.is_empty() => Ok(Self {
                forwarded,
                system,
                value,
            }),
            _ => Err(Unusable::NoIdentifier),
        }
    }

    /// Returns the member's answer, its bytes unchanged.
    #[must_use]
    pub fn forwarded(&self) -> &Forwarded {
        &self.forwarded
    }

    /// Returns the system and the value of the document's
    /// `Bundle.identifier`.
    #[must_use]
    pub fn identifier(&self) -> (&str, &str) {
        (&self.system, &self.value)
    }
}

/// What a document is read for: its type and its `Bundle.identifier`.
#[derive(serde::Deserialize)]
struct Head {
    /// The resource type.
    #[serde(rename = "resourceType")]
    resource_type: Option<String>,
    /// The `Bundle.type`.
    #[serde(rename = "type")]
    kind: Option<String>,
    /// The document's identifier.
    identifier: Option<HeadIdentifier>,
}

/// An identifier's system and value.
#[derive(serde::Deserialize)]
struct HeadIdentifier {
    /// The system.
    system: Option<String>,
    /// The value.
    value: Option<String>,
}

/// Why a member's success is no document; no variant's text names a value
/// the answer carries.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Unusable {
    /// The answer is no JSON object of a FHIR resource's shape.
    #[error("the answer is no FHIR resource in JSON")]
    Unreadable(#[source] serde_json::Error),
    /// The answer is a resource other than a `Bundle`.
    #[error("the answer is no Bundle")]
    NotBundle,
    /// The `Bundle` is of a type other than `document`.
    #[error("the Bundle is no document")]
    NotDocument,
    /// The document carries no identifier with a system and a value
    /// (FHIR R4 `bdl-9`).
    #[error("the document carries no identifier with a system and a value (bdl-9)")]
    NoIdentifier,
}

/// What a member answered the request for its own patient summary.
#[derive(Debug)]
pub enum Answer {
    /// A success that is a FHIR document `Bundle` with an identifier: the
    /// member's document, as it sent it.
    Document(Document),
    /// `404 Not Found`: the member holds no summary for the `ehr_id`.
    Absent,
    /// Any other status, which says nothing about the patient.
    Erred(StatusCode),
    /// A success, of the status `status`, that is no FHIR document `Bundle`
    /// whose identifier has a system and a value (FHIR R4 `bdl-9`), so it
    /// says nothing about the patient either.
    Unusable {
        /// The member's status.
        status: StatusCode,
        /// What the answer misses.
        reason: Unusable,
    },
    /// No answer of the member's: the request was refused at the gateway,
    /// timed out, could not reach the member, or was never sent.
    Failed(ForwardError),
    /// The request was sent and the overall budget ran out before the
    /// member answered (§11.5).
    Abandoned,
}

impl Answer {
    /// Whether the member answered, with its document or with none.
    #[must_use]
    pub fn answered(&self) -> bool {
        matches!(self, Self::Document(_) | Self::Absent)
    }

    /// The §11.1 status of the member: `active` when it answered,
    /// `time-out` when it did not answer in time or the request never got a
    /// slot, `offline` when it could not be reached, and `node-error`
    /// otherwise.
    #[must_use]
    pub fn status(&self) -> EndpointStatus {
        match self {
            Self::Document(_) | Self::Absent => EndpointStatus::Active,
            Self::Abandoned
            | Self::Failed(
                ForwardError::TimeOut { .. }
                | ForwardError::Expired { .. }
                | ForwardError::Capped(_),
            ) => EndpointStatus::TimeOut,
            Self::Failed(ForwardError::Unreachable { .. }) => EndpointStatus::Offline,
            Self::Erred(_) | Self::Unusable { .. } | Self::Failed(_) => EndpointStatus::NodeError,
        }
    }

    /// What the request showed of the member.
    #[must_use]
    pub fn contact(&self) -> Contact {
        match self {
            Self::Document(document) => Contact::Answered(document.forwarded().status()),
            Self::Absent => Contact::Answered(StatusCode::NOT_FOUND),
            Self::Erred(status) | Self::Unusable { status, .. } => Contact::Answered(*status),
            Self::Failed(error) => Contact::of_forward_error(error),
            Self::Abandoned => Contact::Silent,
        }
    }
}

impl<T: Transport + Clone> NodeClient<T> {
    /// Asks the member this client reaches for its own patient summary at
    /// the native endpoint `asked` names, under the deadline of `options`,
    /// and abandons the request when the overall budget `until` runs out
    /// after it left.
    ///
    /// The outbound gate reads the request's path, query and conveyed
    /// claims against the identifiers `options` withholds, and the
    /// conveyance is held to the member's own `ehr_id` (§5.4.1, N33).
    pub async fn native_summary(
        &self,
        asked: SummaryAsked<'_>,
        options: &DispatchOptions,
        until: tokio::time::Instant,
    ) -> Answer {
        let call = async {
            let outgoing = match self.native_outgoing(asked, options) {
                Ok(outgoing) => outgoing,
                Err(refused) => return Some(Err(refused)),
            };
            let client = match self.client_at(asked.native.url().clone(), options) {
                Ok(client) => client,
                Err(source) => {
                    return Some(Err(ForwardError::Compose {
                        endpoint: self.endpoint().clone(),
                        source: Box::new(source),
                    }));
                }
            };
            let slot = match self.slot(options.deadline()).await {
                Ok(slot) => slot,
                Err(capped) => return Some(Err(capped.into())),
            };
            let sent = async {
                let answered = client.forward(outgoing).await;
                drop(slot);
                answered
            };
            // NOTE: §11.5, N38: an elapsed budget is the abandonment `None` reports, as for the
            // ask-all probe; a request that never left ends `Expired`.
            tokio::time::timeout_at(until, sent)
                .await
                .ok()
                .map(|sent| sent.map_err(|error| self.unanswered(error, options)))
        };
        let answered = trace_context::node_request(self.endpoint(), OPERATION, call, |outcome| {
            let contact = match outcome {
                Some(Ok(answer)) => Contact::Answered(answer.status()),
                Some(Err(error)) => Contact::of_forward_error(error),
                None => Contact::Silent,
            };
            (Some(contact), None)
        })
        .await;
        match answered {
            None => Answer::Abandoned,
            Some(Err(failure)) => Answer::Failed(failure),
            Some(Ok(answer)) if answer.status() == StatusCode::UNAUTHORIZED => {
                Answer::Failed(ForwardError::Refused {
                    endpoint: self.endpoint().clone(),
                    status: answer.status(),
                    body: answer.error_body(),
                })
            }
            Some(Ok(answer)) if answer.status().is_success() => {
                let status = answer.status();
                match Document::read(Forwarded::of_answer(answer)) {
                    Ok(document) => Answer::Document(document),
                    Err(reason) => Answer::Unusable { status, reason },
                }
            }
            Some(Ok(answer)) if answer.status() == StatusCode::NOT_FOUND => Answer::Absent,
            Some(Ok(answer)) => Answer::Erred(answer.status()),
        }
    }

    /// The request for the summary `asked` names, with the options of
    /// `options` applied, once the outbound gate admitted it.
    fn native_outgoing(
        &self,
        asked: SummaryAsked<'_>,
        options: &DispatchOptions,
    ) -> Result<Request, ForwardError> {
        options
            .conveyance()
            .holds_own(self.endpoint(), Some(asked.ehr_id))
            .map_err(|source| ForwardError::Conveyance {
                endpoint: self.endpoint().clone(),
                source,
            })?;
        let mut outgoing = Request::new(Method::GET, SUMMARY_PATH.to_owned());
        outgoing.query(
            "identifier",
            format!("{}|{}", asked.native.ehr_id_system(), asked.ehr_id),
        );
        outgoing.query("profile", asked.profile);
        outgoing
            .headers_mut()
            .insert(ACCEPT, HeaderValue::from_static(FHIR_JSON));
        // NOTE: §13.1, N24, §8.4: the caller is signed for the member's Query API endpoint, whose
        // credential the request carries, since no directive names a native endpoint.
        let call = options
            .call_options(self.endpoint())
            .map_err(|error| match error {
                OptionsError::Conveyance(source) => ForwardError::Conveyance {
                    endpoint: self.endpoint().clone(),
                    source,
                },
                OptionsError::Client(source) => ForwardError::Compose {
                    endpoint: self.endpoint().clone(),
                    source: Box::new(source),
                },
            })?;
        outgoing.apply_options(&call);
        self.gate_native(asked, &outgoing, options)?;
        Ok(outgoing)
    }

    /// The outbound gate over the request for a member's own summary: its
    /// URL, the caller claims it conveys and the `Accept` it sets (§5.4.1,
    /// N33).
    ///
    /// The query is searched whole, the member's `ehr_id` included: an
    /// identifier that occurs inside it refuses the request, which then
    /// fails closed.
    fn gate_native(
        &self,
        asked: SummaryAsked<'_>,
        request: &Request,
        options: &DispatchOptions,
    ) -> Result<(), ForwardError> {
        let withheld = options.withheld();
        if withheld.is_empty() {
            return Ok(());
        }
        let base = asked.native.url();
        let mut url = base.clone();
        url.set_path(&format!(
            "{}{}",
            base.path().trim_end_matches('/'),
            request.path()
        ));
        let query = request.query_string();
        url.set_query((!query.is_empty()).then_some(query));
        let conveyed = options.conveyance().carried();
        let outbound = Outbound {
            aql: "",
            scope: None,
            paging: &[],
            url: &url,
            composed: Composed::default(),
            headers: &[("accept", FHIR_JSON)],
            conveyed: &conveyed,
        };
        match withheld.found_in(&outbound) {
            Some(part) => Err(ForwardError::Withheld {
                endpoint: self.endpoint().clone(),
                part,
            }),
            None => Ok(()),
        }
    }
}
