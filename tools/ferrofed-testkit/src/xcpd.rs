// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A stub XCPD Responding Gateway: a test device that answers ITI-55 Cross
//! Gateway Patient Discovery for the patients it is told about (ITI TF-2
//! §3.55, Revision 20.1).
//!
//! Each patient, by the identifier value its request names, gets one
//! [`Answer`]: the communities that hold it (Cases 1 and 2 of §3.55.4.2.3),
//! no match (Case 4), a busy responder (Case 5), a SOAP fault, or silence.
//! The stub reads each request and writes each answer with the Responding
//! Gateway messages of `ihe-iti`, FerroPIX's crate
//! ([`ihe_iti::xcpd::responding`]): a `PRPA_IN201306UV02` whose `RelatesTo`
//! is the request's `MessageID`, or a SOAP 1.2 fault, every identifier in it
//! synthetic. The stub speaks plain `http`, so a
//! client reaches it only through the development path of its configuration.
//! No specification governs the stub: our own design.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use ihe_iti::xcpd::error::DetectedIssue;
use ihe_iti::xcpd::identifier::{HomeCommunityId, Oid};
use ihe_iti::xcpd::responding::answer::{self, Blame, Candidate, Outcome, Responder};
use ihe_iti::xcpd::responding::query::{self, ReceivedQuery};
use secrecy::{ExposeSecret as _, SecretString};
use wiremock::matchers::{method, path};
use wiremock::{Mock, Request, Respond, ResponseTemplate};

use crate::mock::Server;

/// The path the stub serves ITI-55 on.
pub const PATH: &str = "/RespondingGateway";

/// The stub's device, the sender of every answer.
const DEVICE: &str = "2.999.50.1";

/// The stub's own community.
const COMMUNITY: &str = "2.999.50";

/// One community that holds the patient: its `homeCommunityId` OID and the
/// patient's identifier there.
#[derive(Debug, Clone)]
pub struct Community {
    /// The community's OID, in dotted form.
    pub home: String,
    /// The assigning authority of the patient's identifier in it.
    pub authority: String,
    /// The patient's identifier in it.
    pub patient: String,
}

impl Community {
    /// The community `home`, holding the patient as `patient` of
    /// `authority`.
    #[must_use]
    pub fn new(home: &str, authority: &str, patient: &str) -> Self {
        Self {
            home: home.to_owned(),
            authority: authority.to_owned(),
            patient: patient.to_owned(),
        }
    }
}

/// What the stub answers for one patient.
#[derive(Debug, Clone)]
pub enum Answer {
    /// One registration event per community (Cases 1 and 2).
    Holds(Vec<Community>),
    /// No match (Case 4).
    NoMatch,
    /// The responder is busy (Case 5).
    Busy,
    /// A SOAP 1.2 `Receiver` fault with a `500`.
    Fault,
    /// No answer within `30` seconds.
    Silent,
}

/// A running stub responding gateway.
#[derive(Debug)]
pub struct RespondingGateway {
    server: Server,
}

impl RespondingGateway {
    /// Starts a gateway answering `answers` by patient identifier value, and
    /// `otherwise` for any other patient.
    pub async fn start(answers: BTreeMap<String, Answer>, otherwise: Answer) -> Self {
        let server = Server::start().await;
        Mock::given(method("POST"))
            .and(path(PATH))
            .respond_with(Answers {
                answers: Arc::new(answers),
                otherwise,
            })
            .mount(&server)
            .await;
        Self { server }
    }

    /// Starts a gateway answering `answer` for every patient.
    pub async fn answering(answer: Answer) -> Self {
        Self::start(BTreeMap::new(), answer).await
    }

    /// The gateway's SOAP endpoint, an `http` URL.
    #[must_use]
    pub fn endpoint(&self) -> String {
        format!("{}{PATH}", self.server.uri())
    }

    /// The body of every request the gateway received, in order.
    ///
    /// # Errors
    ///
    /// Returns [`RecordingOff`](crate::mock::RecordingOff) when the device's
    /// server records no requests.
    pub async fn requests(&self) -> Result<Vec<String>, crate::mock::RecordingOff> {
        Ok(crate::mock::recorded(&self.server)
            .await?
            .iter()
            .map(|request| String::from_utf8_lossy(&request.body).into_owned())
            .collect())
    }
}

/// The wiremock responder behind a [`RespondingGateway`].
struct Answers {
    answers: Arc<BTreeMap<String, Answer>>,
    otherwise: Answer,
}

impl Respond for Answers {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let media = request
            .headers
            .get(http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok());
        let Ok(received) = query::read(media, &request.body) else {
            return written(400, answer::fault(Blame::Sender, "no ITI-55 query"));
        };
        let answer = patient(&received)
            .and_then(|patient| self.answers.get(&patient))
            .unwrap_or(&self.otherwise);
        let outcome = match answer {
            Answer::Holds(communities) => match candidates(communities) {
                Some(candidates) => Outcome::Matched(candidates),
                None => {
                    return written(500, answer::fault(Blame::Receiver, "a community is no OID"));
                }
            },
            Answer::NoMatch => Outcome::NoMatch,
            Answer::Busy => Outcome::Unable(vec![DetectedIssue::ResponderBusy]),
            Answer::Fault => {
                return written(500, answer::fault(Blame::Receiver, "synthetic outage"));
            }
            Answer::Silent => {
                return ResponseTemplate::new(200).set_delay(Duration::from_secs(30));
            }
        };
        let Some(responder) = responder() else {
            return written(
                500,
                answer::fault(Blame::Receiver, "the stub's device is no OID"),
            );
        };
        written(200, answer::envelope(&received, &responder, &outcome))
    }
}

/// The value of the first identifier the query names, the one the stub
/// answers by.
fn patient(received: &ReceivedQuery) -> Option<String> {
    received
        .parameters
        .ids
        .first()
        .map(|id| id.value().expose_secret().to_owned())
}

/// One matching record per community (§3.55.4.2.2.2, §3.55.4.2.2.4), or
/// `None` when a community or an authority is no OID.
fn candidates(communities: &[Community]) -> Option<Vec<Candidate>> {
    communities
        .iter()
        .map(|community| {
            Some(Candidate {
                community: HomeCommunityId::new(Oid::new(&community.home).ok()?),
                patient_id: (
                    Oid::new(&community.authority).ok()?,
                    SecretString::from(community.patient.clone()),
                ),
                other_ids: Vec::new(),
                names: Vec::new(),
                gender: None,
                birth_time: None,
                addresses: Vec::new(),
            })
        })
        .collect()
}

/// The stub, as its answers name it.
fn responder() -> Option<Responder> {
    Some(Responder {
        device: Oid::new(DEVICE).ok()?,
        community: HomeCommunityId::new(Oid::new(COMMUNITY).ok()?),
    })
}

/// The answer `written` with `status`, or a `500` when it could not be
/// written.
fn written(status: u16, written: std::io::Result<Vec<u8>>) -> ResponseTemplate {
    match written {
        Ok(body) => ResponseTemplate::new(status).set_body_raw(body, answer::MEDIA_TYPE),
        Err(_unwritten) => ResponseTemplate::new(500),
    }
}
