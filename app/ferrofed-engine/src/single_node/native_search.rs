// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The search of a member's own exchange-format endpoint: one page of a
//! FHIR R4 search-type interaction over one resource type, and the
//! member's answer returned as it sent it.
//!
//! The first page is `GET {fhir}/{type}?patient.identifier=<system>|<ehr_id>`,
//! naming the patient by the member's own `ehr_id` in the identifier system
//! the registry declares for it, the one value about the patient the request
//! carries (§5.4.1, N33); a search for one resource adds its logical id as
//! `_id` (FHIR R4 search §3.1.1.4.1), so a read stays held to the patient.
//! For a member that takes no chained parameter, the patient is found by
//! `Patient?identifier=<system>|<ehr_id>` and its resources by `patient` and
//! `type`, and a resource it names is read by its logical id, the
//! interactions the HL7 Europe EU Health Data API 1.0.0-ballot Document
//! Access Provider SHALL support ([`By`]). A later page is the `next` link the member's own
//! `searchset` gave (FHIR R4 `http.html` §3.1.0.14 Paging), and only a link
//! under the member's own base can be named ([`Next`]). Every page leaves as
//! the patient summary does ([`super::native`]): through the member's Query
//! API client, with its onward credentials, the caller's identity signed for
//! that endpoint, the minted `X-Request-Id` and a slot of its in-flight cap,
//! past the same outbound gate, which reads its path, its query and the
//! caller claims it conveys against the identifiers resolution consumed. The
//! search form a member is asked in, and following its links, are our own
//! design: no specification governs them.

use ferrofed_registry::id::EhrId;
use ferrofed_registry::native::NativeEndpoint;
use http::{Method, StatusCode};
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::client::{Request, Transport};
use url::Url;

use crate::dispatch::{Contact, DispatchOptions, NodeClient};
use crate::single_node::forward::{ForwardError, Forwarded};

/// The operation a native search is traced under.
const OPERATION: &str = "native_search";

/// The search parameter that names the patient, chained to its identifier
/// (FHIR R4 search §3.1.1.4.15 Chained parameters).
pub const PATIENT_IDENTIFIER: &str = "patient.identifier";

/// The search parameter that names one resource by its logical id (FHIR R4
/// search §3.1.1.4.1 Parameters for all resources).
pub const ID: &str = "_id";

/// The search parameter that names a `Patient` by its identifier (FHIR R4
/// Patient `identifier`).
pub const IDENTIFIER: &str = "identifier";

/// The search parameter that names the patient by reference (FHIR R4
/// `DocumentReference` `patient`).
pub const PATIENT: &str = "patient";

/// The search parameter that names the document type (FHIR R4
/// `DocumentReference` `type`).
pub const TYPE: &str = "type";

/// A member's `next` link, held to the member's own base: the path below
/// the base and the query it names.
///
/// `Debug` shows neither, since the query is the member's own state and may
/// carry anything.
#[derive(Clone, PartialEq, Eq)]
pub struct Next {
    path: String,
    query: Option<String>,
}

impl std::fmt::Debug for Next {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Next").finish_non_exhaustive()
    }
}

/// Why a member's `next` link is not followed; no variant's text names a
/// value the link carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Elsewhere {
    /// The link leaves the member's own endpoint: another scheme, host or
    /// port, user information or a fragment, or a path outside its base.
    #[error("the member's next link leaves its own endpoint")]
    Outside,
}

impl Next {
    /// Reads `next`, a member's `next` link, as a page under its base
    /// `base`.
    ///
    /// # Errors
    ///
    /// [`Elsewhere::Outside`] for a link of another origin, one with user
    /// information or a fragment, or one whose path is not the base or below
    /// it.
    pub fn under(base: &Url, next: &Url) -> Result<Self, Elsewhere> {
        if base.origin() != next.origin()
            || !next.username().is_empty()
            || next.password().is_some()
            || next.fragment().is_some()
        {
            return Err(Elsewhere::Outside);
        }
        let root = base.path().trim_end_matches('/');
        let rest = next.path().strip_prefix(root).ok_or(Elsewhere::Outside)?;
        if !(rest.is_empty() || rest.starts_with('/')) {
            return Err(Elsewhere::Outside);
        }
        Ok(Self {
            path: rest.to_owned(),
            query: next.query().map(str::to_owned),
        })
    }

    /// Returns the path below the member's base.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// How the first page of a search names what it asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum By<'a> {
    /// `{type}?patient.identifier=<system>|<ehr_id>`, with `_id` when the
    /// logical id of one resource is given.
    PatientIdentifier(Option<&'a str>),
    /// `{type}?identifier=<system>|<ehr_id>`: the member's own `Patient`
    /// for its `ehr_id` (FHIR R4 Patient `identifier`).
    Identifier,
    /// `{type}?patient=<patient>&type=<kind>`: the resources of the member's
    /// `Patient` of the logical id `patient`, of the type token `kind`
    /// (FHIR R4 `DocumentReference` `patient` and `type`).
    Patient {
        /// The logical id of the member's `Patient`.
        patient: &'a str,
        /// The type token, `system|code`.
        kind: &'a str,
    },
    /// `{type}/<id>`: the read of the one resource of the logical id `id`
    /// (FHIR R4 `http.html` §3.1.0.2), which a member answers with no page.
    Read(&'a str),
}

/// What one page of a search of a member's own endpoint asks.
#[derive(Debug, Clone, Copy)]
pub struct SearchAsked<'a> {
    /// The member's native endpoint.
    pub native: &'a NativeEndpoint,
    /// The member's own `ehr_id`, the one value about the patient the
    /// request carries.
    pub ehr_id: &'a EhrId,
    /// The resource type searched, such as `MedicationRequest`.
    pub resource: &'a str,
    /// How the first page names what it asks for.
    pub by: By<'a>,
    /// The `next` link of the member's previous page, or `None` for the
    /// first page.
    pub next: Option<&'a Next>,
}

/// What a member answered one page of a search with.
#[derive(Debug)]
pub enum Searched {
    /// A success: the member's page, as it sent it.
    Page(Forwarded),
    /// Any other status, which says nothing about the patient (FHIR R4
    /// `http.html` §3.1.0.9: a search that cannot be executed answers `4xx`
    /// or `5xx`).
    Erred(StatusCode),
    /// No answer of the member's: the request was refused at the gateway,
    /// timed out, could not reach the member, or was never sent.
    Failed(ForwardError),
    /// The request was sent and the overall budget ran out before the
    /// member answered (§11.5).
    Abandoned,
}

impl Searched {
    /// The §11.1 status of the member: `active` when it answered with a
    /// page, `time-out` when it did not answer in time or the request never
    /// got a slot, `offline` when it could not be reached, and `node-error`
    /// otherwise.
    #[must_use]
    pub fn status(&self) -> EndpointStatus {
        match self {
            Self::Page(_) => EndpointStatus::Active,
            Self::Abandoned
            | Self::Failed(
                ForwardError::TimeOut { .. } | ForwardError::Expired(_) | ForwardError::Capped(_),
            ) => EndpointStatus::TimeOut,
            Self::Failed(ForwardError::Unreachable { .. }) => EndpointStatus::Offline,
            Self::Erred(_) | Self::Failed(_) => EndpointStatus::NodeError,
        }
    }

    /// What the request showed of the member.
    #[must_use]
    pub fn contact(&self) -> Contact {
        match self {
            Self::Page(page) => Contact::Answered(page.status()),
            Self::Erred(status) => Contact::Answered(*status),
            Self::Failed(error) => Contact::of_forward_error(error),
            Self::Abandoned => Contact::Silent,
        }
    }
}

impl<T: Transport + Clone> NodeClient<T> {
    /// Asks the member this client reaches for one page of the search
    /// `asked` names, at its native endpoint, under the deadline of
    /// `options`, and abandons the request when the overall budget `until`
    /// runs out after it left.
    ///
    /// The outbound gate reads the request's path, query and conveyed
    /// claims against the identifiers `options` withholds, and the
    /// conveyance is held to the member's own `ehr_id` (§5.4.1, N33).
    pub async fn native_search(
        &self,
        asked: SearchAsked<'_>,
        options: &DispatchOptions,
        until: tokio::time::Instant,
    ) -> Searched {
        let outgoing =
            self.native_admitted(asked.native, asked.ehr_id, search_outgoing(asked), options);
        let answered = self
            .native_sent(asked.native, outgoing, (options, until), OPERATION)
            .await;
        match answered {
            None => Searched::Abandoned,
            Some(Err(failure)) => Searched::Failed(failure),
            Some(Ok(answer)) if answer.status() == StatusCode::UNAUTHORIZED => {
                Searched::Failed(ForwardError::Refused {
                    endpoint: self.endpoint().clone(),
                    status: answer.status(),
                    body: answer.error_body(),
                })
            }
            Some(Ok(answer)) if answer.status().is_success() => {
                Searched::Page(Forwarded::of_answer(answer))
            }
            Some(Ok(answer)) => Searched::Erred(answer.status()),
        }
    }
}

/// The request for the page `asked` names, before its options and its
/// gate.
fn search_outgoing(asked: SearchAsked<'_>) -> Request {
    match asked.next {
        None => {
            let own = format!("{}|{}", asked.native.ehr_id_system(), asked.ehr_id);
            let mut outgoing = match asked.by {
                By::Read(id) => Request::new(Method::GET, format!("/{}/{id}", asked.resource)),
                _ => Request::new(Method::GET, format!("/{}", asked.resource)),
            };
            match asked.by {
                By::PatientIdentifier(id) => {
                    outgoing.query(PATIENT_IDENTIFIER, own);
                    if let Some(id) = id {
                        outgoing.query(ID, id);
                    }
                }
                By::Identifier => outgoing.query(IDENTIFIER, own),
                By::Patient { patient, kind } => {
                    outgoing.query(PATIENT, patient);
                    outgoing.query(TYPE, kind);
                }
                By::Read(_) => {}
            }
            outgoing
        }
        Some(next) => {
            let mut outgoing = Request::new(Method::GET, next.path.clone());
            if let Some(query) = &next.query {
                outgoing.raw_query(query);
            }
            outgoing
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Elsewhere, Next};
    use url::Url;

    #[test]
    fn a_link_under_the_base_is_followed_by_its_path() {
        let base = Url::parse("https://member.example.org/fhir").unwrap();
        for (next, path) in [
            ("https://member.example.org/fhir?_getpages=a1&page=2", ""),
            (
                "https://member.example.org/fhir/MedicationRequest?page=2",
                "/MedicationRequest",
            ),
        ] {
            let read = Next::under(&base, &Url::parse(next).unwrap()).unwrap();
            assert_eq!(path, read.path(), "{next}");
        }
    }

    #[test]
    fn a_link_that_leaves_the_base_is_refused() {
        let base = Url::parse("https://member.example.org/fhir/").unwrap();
        for next in [
            "http://member.example.org/fhir/MedicationRequest",
            "https://other.example.org/fhir/MedicationRequest",
            "https://member.example.org:8443/fhir/MedicationRequest",
            "https://member.example.org/fhirx/MedicationRequest",
            "https://member.example.org/other",
            "https://user@member.example.org/fhir/MedicationRequest",
            "https://member.example.org/fhir/MedicationRequest#page",
        ] {
            assert_eq!(
                Err(Elsewhere::Outside),
                Next::under(&base, &Url::parse(next).unwrap()),
                "{next}"
            );
        }
    }
}
