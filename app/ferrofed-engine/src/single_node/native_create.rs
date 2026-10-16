// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The create of one resource at a member's own exchange-format endpoint.
//!
//! It is one FHIR R4 `create` interaction, `POST {fhir}/{type}`, sent once,
//! and the member's answer is returned as it sent it (FHIR R4 `http.html`
//! §3.1.0.8).
//!
//! The body is the client's resource, sent byte for byte as it was received:
//! a write body is clinical content the client supplied, which the gateway
//! never alters or reads for identifiers (§5.4 scope note, N33). Everything
//! else the gateway composes leaves as the patient summary's request does
//! ([`super::native`]): through the member's Query API client, with its
//! onward credentials, the caller's identity signed for that endpoint and
//! held to the member's own `ehr_id`, the minted `X-Request-Id` and a slot
//! of its in-flight cap, past the outbound gate, which reads the path, the
//! query and the conveyed claims against the identifiers resolution consumed.
//!
//! The request is sent once and never again, whatever the member answers or
//! fails to: a create that left may have been made, so a second one could
//! write twice. No specification governs how a member's answer is read: our
//! own design.

use ferrofed_registry::id::EhrId;
use ferrofed_registry::native::NativeEndpoint;
use http::{HeaderValue, Method, StatusCode};
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::client::{Request, Transport};

use crate::dispatch::{Contact, DispatchOptions, NodeClient};
use crate::single_node::forward::{ForwardError, Forwarded};
use crate::single_node::native::FHIR_JSON;

/// The operation a native create is traced under.
const OPERATION: &str = "native_create";

/// What one create at a member's own endpoint asks.
///
/// `Debug` shows the resource type and the body's size, never the body.
#[derive(Clone, Copy)]
pub struct CreateAsked<'a> {
    /// The member's native endpoint.
    pub native: &'a NativeEndpoint,
    /// The member's own `ehr_id` for the patient, which the caller's
    /// conveyed identity is held to.
    pub ehr_id: &'a EhrId,
    /// The resource type created, such as `MedicationDispense`.
    pub resource: &'a str,
    /// The resource, as the client sent it.
    pub body: &'a [u8],
}

impl std::fmt::Debug for CreateAsked<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreateAsked")
            .field("resource", &self.resource)
            .field("body_bytes", &self.body.len())
            .finish_non_exhaustive()
    }
}

/// What a member answered a create with.
#[derive(Debug)]
pub enum Created {
    /// A success: the member's answer, as it sent it (FHIR R4 `http.html`
    /// §3.1.0.8: `201 Created` with a `Location`).
    Made(Forwarded),
    /// A client-error status other than `401`: the member refused the
    /// resource (FHIR R4 `http.html` §3.1.0.8: `400`, `422`), or its own
    /// state no longer admits it.
    Refused(StatusCode),
    /// Any other status the member answered with.
    Erred(StatusCode),
    /// No answer of the member's: the request was refused at the gateway,
    /// timed out, could not reach the member, or was never sent.
    Failed(ForwardError),
    /// The request was sent and the overall budget ran out before the
    /// member answered (§11.5).
    Abandoned,
}

impl Created {
    /// The §11.1 status of the member: `active` when it answered with a
    /// success or a refusal, `time-out` when it did not answer in time or
    /// the request never got a slot, `offline` when it could not be reached,
    /// and `node-error` otherwise.
    #[must_use]
    pub fn status(&self) -> EndpointStatus {
        match self {
            Self::Made(_) | Self::Refused(_) => EndpointStatus::Active,
            Self::Abandoned
            | Self::Failed(
                ForwardError::TimeOut { .. }
                | ForwardError::Expired { .. }
                | ForwardError::Capped(_),
            ) => EndpointStatus::TimeOut,
            Self::Failed(ForwardError::Unreachable { .. }) => EndpointStatus::Offline,
            Self::Erred(_) | Self::Failed(_) => EndpointStatus::NodeError,
        }
    }

    /// What the request showed of the member.
    #[must_use]
    pub fn contact(&self) -> Contact {
        match self {
            Self::Made(made) => Contact::Answered(made.status()),
            Self::Refused(status) | Self::Erred(status) => Contact::Answered(*status),
            Self::Failed(error) => Contact::of_forward_error(error),
            Self::Abandoned => Contact::Silent,
        }
    }
}

impl<T: Transport + Clone> NodeClient<T> {
    /// Asks the member this client reaches to create the resource `asked`
    /// carries at its native endpoint, once, under the deadline of
    /// `options`, and abandons the request when the overall budget `until`
    /// runs out after it left.
    ///
    /// The outbound gate reads the request's path, query and conveyed
    /// claims against the identifiers `options` withholds, and the
    /// conveyance is held to the member's own `ehr_id` (§5.4.1, N33); the
    /// body is never read (§5.4 scope note).
    pub async fn native_create(
        &self,
        asked: CreateAsked<'_>,
        options: &DispatchOptions,
        until: tokio::time::Instant,
    ) -> Created {
        let mut outgoing = Request::new(Method::POST, format!("/{}", asked.resource));
        outgoing.raw_body(
            asked.body.to_vec(),
            Some(HeaderValue::from_static(FHIR_JSON)),
        );
        let outgoing = self.native_admitted(asked.native, asked.ehr_id, outgoing, options);
        let answered = self
            .native_sent(asked.native, outgoing, (options, until), OPERATION)
            .await;
        match answered {
            None => Created::Abandoned,
            Some(Err(ForwardError::Oversized {
                status, endpoint, ..
            })) if status.is_success() => {
                tracing::warn!(
                    endpoint = %endpoint,
                    "a member created a resource and answered with a body longer than the gateway reads"
                );
                Created::Made(Forwarded::bare(status))
            }
            Some(Err(failure)) => Created::Failed(failure),
            Some(Ok(answer)) if answer.status() == StatusCode::UNAUTHORIZED => {
                Created::Failed(ForwardError::Refused {
                    endpoint: self.endpoint().clone(),
                    status: answer.status(),
                    body: answer.error_body(),
                })
            }
            Some(Ok(answer)) if answer.status().is_success() => {
                Created::Made(Forwarded::of_answer(answer))
            }
            Some(Ok(answer)) if answer.status().is_client_error() => {
                Created::Refused(answer.status())
            }
            Some(Ok(answer)) => Created::Erred(answer.status()),
        }
    }
}
