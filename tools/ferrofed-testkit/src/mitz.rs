// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stub Mitz of the Dutch Generic Functions: it answers the closed
//! authorization question (Annex B §B.6).
//!
//! **This is a test device, not Mitz.** It reads the BSN, the data holder's
//! URA and each data category from a question in the shape of the
//! Implementatiehandleiding Open en gesloten autorisatievraag 3.8.2
//! (§3.2.4.4), and answers an XACML 3.0 `Response` in the shape of its
//! example (§3.2.5.4): one `Result` per category, echoing the patient, the
//! holder and the category. Every decision is `Permit` unless a test denies
//! a patient at a holder. It keeps no consent and applies no rule of the
//! afsprakenstelsel. No specification governs the device: our own design.
//!
//! Every value a test gives it is synthetic, and nothing here logs one; its
//! `Debug` shows counts only.

use std::collections::BTreeSet;
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use wiremock::matchers::{method, path};
use wiremock::{Mock, Request, Respond, ResponseTemplate};

use crate::mock::Server;

/// The path the device answers on.
const ENDPOINT: &str = "/mitz/geslotenautorisatievraag";

/// The SOAP 1.2 media type of every answer.
const SOAP_XML: &str = "application/soap+xml; charset=utf-8";

/// The attribute identifiers the device reads (§3.2.4.2).
const PATIENT_ID: &str = "urn:oasis:names:tc:xacml:2.0:resource:resource-id";
const HOLDER_ID: &str = "urn:ihe:iti:appc:2016:author-institution:id";
const CATEGORY: &str = "urn:ihe:iti:appc:2016:document-entry:event-code";

/// What the device answers besides its decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outage {
    /// Every question is a `503`.
    Refusing,
    /// Every question waits a minute before it answers.
    Silent,
    /// Every question is answered `Indeterminate`.
    Indeterminate,
}

/// What the device holds.
#[derive(Debug, Default)]
struct State {
    /// The data holder URAs whose questions answer `503`.
    failing: BTreeSet<String>,
    /// Each patient and data holder URA Mitz denies every category for.
    denied: BTreeSet<(String, String)>,
    /// The outage the device plays, if any.
    outage: Option<Outage>,
}

/// The stub Mitz.
pub struct Mitz {
    server: Server,
    state: Arc<Mutex<State>>,
}

impl Mitz {
    /// Starts the device, permitting everything.
    pub async fn start() -> Self {
        let server = Server::start().await;
        let state = Arc::new(Mutex::new(State::default()));
        Mock::given(method("POST"))
            .and(path(ENDPOINT))
            .respond_with(Answer(Arc::clone(&state)))
            .mount(&server)
            .await;
        Self { server, state }
    }

    /// The device's endpoint URL.
    #[must_use]
    pub fn endpoint(&self) -> String {
        format!("{}{ENDPOINT}", self.server.uri())
    }

    /// Denies every category of `patient`'s data at the data holder `ura`.
    pub fn deny(&self, patient: &str, ura: &str) {
        self.lock()
            .denied
            .insert((patient.to_owned(), ura.to_owned()));
    }

    /// Makes every question about the data holder `ura` answer `503`.
    pub fn refuse_at(&self, ura: &str) {
        self.lock().failing.insert(ura.to_owned());
    }

    /// Makes every question answer `503`.
    pub fn refuse(&self) {
        self.lock().outage = Some(Outage::Refusing);
    }

    /// Makes every question wait a minute before it answers.
    pub fn go_silent(&self) {
        self.lock().outage = Some(Outage::Silent);
    }

    /// Makes every question answer `Indeterminate`.
    pub fn answer_indeterminate(&self) {
        self.lock().outage = Some(Outage::Indeterminate);
    }

    /// The body of every question the device received, in order.
    pub async fn questions(&self) -> Vec<String> {
        self.server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .map(|request| String::from_utf8_lossy(&request.body).into_owned())
            .collect()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl fmt::Debug for Mitz {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = self.lock();
        f.debug_struct("Mitz")
            .field("denied", &state.denied.len())
            .field("outage", &state.outage)
            .finish_non_exhaustive()
    }
}

/// The responder every question reaches.
struct Answer(Arc<Mutex<State>>);

impl Respond for Answer {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let state = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        match state.outage {
            Some(Outage::Refusing) => return ResponseTemplate::new(503),
            Some(Outage::Silent) => {
                return ResponseTemplate::new(200).set_delay(Duration::from_secs(60));
            }
            Some(Outage::Indeterminate) | None => {}
        }
        let body = String::from_utf8_lossy(&request.body);
        let (Some(patient), Some(holder)) = (
            value(&body, PATIENT_ID, "extension"),
            value(&body, HOLDER_ID, "extension"),
        ) else {
            return ResponseTemplate::new(400);
        };
        let categories = values(&body, CATEGORY, "code");
        if categories.is_empty() {
            return ResponseTemplate::new(400);
        }
        if state.failing.contains(&holder) {
            return ResponseTemplate::new(503);
        }
        let decision = if state.outage == Some(Outage::Indeterminate) {
            "Indeterminate"
        } else if state.denied.contains(&(patient.clone(), holder.clone())) {
            "Deny"
        } else {
            "Permit"
        };
        let results: String = categories
            .iter()
            .map(|category| result(decision, category, &patient, &holder))
            .collect();
        ResponseTemplate::new(200).set_body_raw(envelope(&results), SOAP_XML)
    }
}

/// The first value of `attribute` on an element inside the `Attribute`
/// named `id`.
fn value(body: &str, id: &str, attribute: &str) -> Option<String> {
    values(body, id, attribute).into_iter().next()
}

/// The value of `attribute` on the element inside each `Attribute` named
/// `id`, in order.
fn values(body: &str, id: &str, attribute: &str) -> Vec<String> {
    let opening = format!("AttributeId=\"{id}\"");
    let key = format!("{attribute}=\"");
    body.split(&opening)
        .skip(1)
        .filter_map(|rest| {
            let inner = rest.split("Attribute>").next()?;
            let (_, after) = inner.split_once(&key)?;
            let (found, _) = after.split_once('"')?;
            Some(found.to_owned())
        })
        .collect()
}

/// One `Result` of the answer, as the example of §3.2.5.4 writes it.
fn result(decision: &str, category: &str, patient: &str, holder: &str) -> String {
    format!(
        r#"<Result><Decision>{decision}</Decision><Attributes Category="urn:oasis:names:tc:xacml:3.0:attribute-category:resource" xml:id="resource"><Attribute AttributeId="{PATIENT_ID}" IncludeInResult="true"><AttributeValue DataType="urn:hl7-org:v3#II"><ns9:InstanceIdentifier root="2.16.840.1.113883.2.4.6.3" extension="{patient}" xmlns:ns9="urn:hl7-org:v3"/></AttributeValue></Attribute><Attribute AttributeId="{HOLDER_ID}" IncludeInResult="true"><AttributeValue DataType="urn:hl7-org:v3#II"><ns9:InstanceIdentifier root="2.16.528.1.1007.3.3" extension="{holder}" xmlns:ns9="urn:hl7-org:v3"/></AttributeValue></Attribute></Attributes><Attributes Category="urn:oasis:names:tc:xacml:3.0:attribute-category:action" xml:id="action"><Attribute AttributeId="{CATEGORY}" IncludeInResult="true"><AttributeValue DataType="urn:hl7-org:v3#CV"><ns9:CodedValue code="{category}" codeSystem="2.16.840.1.113883.2.4.3.111.5.10.1" xmlns:ns9="urn:hl7-org:v3"/></AttributeValue></Attribute></Attributes></Result>"#
    )
}

/// The SOAP 1.2 envelope around an XACML 3.0 `Response` of `results`.
fn envelope(results: &str) -> Vec<u8> {
    format!(
        r#"<s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope"><s:Body><Response xmlns="urn:oasis:names:tc:xacml:3.0:core:schema:wd-17">{results}</Response></s:Body></s:Envelope>"#
    )
    .into_bytes()
}
