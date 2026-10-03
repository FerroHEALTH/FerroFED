// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A stub XCPD Responding Gateway: a test device that answers ITI-55 Cross
//! Gateway Patient Discovery for the patients it is told about (ITI TF-2
//! §3.55, Revision 20.1).
//!
//! Each patient, by the identifier value its request names, gets one
//! [`Answer`]: the communities that hold it (Cases 1 and 2 of §3.55.4.2.3),
//! no match (Case 4), a busy responder (Case 5), a SOAP fault, or silence.
//! Every answer is a `PRPA_IN201306UV02` or a SOAP 1.2 fault shaped after
//! the examples of §3.55, its `RelatesTo` the request's `MessageID`, and
//! every identifier in it synthetic. The stub speaks plain `http`, so a
//! client reaches it only through the development path of its configuration.
//! No specification governs the stub: our own design.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Arc;
use std::time::Duration;

use wiremock::matchers::{method, path};
use wiremock::{Mock, Request, Respond, ResponseTemplate};

use crate::mock::Server;

/// The path the stub serves ITI-55 on.
pub const PATH: &str = "/RespondingGateway";

/// The SOAP 1.2 media type of every answer.
const SOAP_XML: &str = "application/soap+xml; charset=UTF-8";

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
            .respond_with(Responder {
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
    pub async fn requests(&self) -> Vec<String> {
        self.server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .map(|request| String::from_utf8_lossy(&request.body).into_owned())
            .collect()
    }
}

/// The wiremock responder behind a [`RespondingGateway`].
struct Responder {
    answers: Arc<BTreeMap<String, Answer>>,
    otherwise: Answer,
}

impl Respond for Responder {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let sent = String::from_utf8_lossy(&request.body);
        let message = between(&sent, "<wsa:MessageID>", "</wsa:MessageID>").unwrap_or_default();
        let patient = between(&sent, "<livingSubjectId>", "</livingSubjectId>")
            .and_then(|parameter| between(parameter, "extension=\"", "\""))
            .unwrap_or_default();
        let answer = self.answers.get(patient).unwrap_or(&self.otherwise);
        let (status, body) = match answer {
            Answer::Holds(communities) => {
                (200, response(message, "AA", "OK", &events(communities), ""))
            }
            Answer::NoMatch => (200, response(message, "AA", "NF", "", "")),
            Answer::Busy => (200, response(message, "AE", "AE", "", BUSY)),
            Answer::Fault => (500, fault(message)),
            Answer::Silent => {
                return ResponseTemplate::new(200).set_delay(Duration::from_secs(30));
            }
        };
        ResponseTemplate::new(status).set_body_raw(body.into_bytes(), SOAP_XML)
    }
}

/// The text of `haystack` between `open` and the `close` after it.
fn between<'a>(haystack: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = haystack.find(open)? + open.len();
    let end = haystack.get(start..)?.find(close)? + start;
    haystack.get(start..end)
}

/// The `reasonOf` of a busy responder (§3.55.4.2.2.7).
const BUSY: &str = r#"<reasonOf typeCode="RSON"><detectedIssueEvent classCode="ALRT" moodCode="EVN"><code code="_ActAdministrativeDetectedIssueManagementCode" codeSystem="2.16.840.1.113883.5.4"/><mitigatedBy typeCode="MITGT"><detectedIssueManagement classCode="ACT" moodCode="EVN"><code code="ResponderBusy" codeSystem="1.3.6.1.4.1.19376.1.2.27.3"/></detectedIssueManagement></mitigatedBy></detectedIssueEvent></reasonOf>"#;

/// One registration event per community (§3.55.4.2.2.2, §3.55.4.2.2.4).
fn events(communities: &[Community]) -> String {
    let mut text = String::new();
    for community in communities {
        let Community {
            home,
            authority,
            patient,
        } = community;
        // NOTE: writing to a String cannot fail, so the result is dropped.
        let _written: std::fmt::Result = write!(
            text,
            r#"<subject typeCode="SUBJ"><registrationEvent classCode="REG" moodCode="EVN"><id nullFlavor="NA"/><statusCode code="active"/><subject1 typeCode="SBJ"><patient classCode="PAT"><id root="{authority}" extension="{patient}"/><statusCode code="active"/><patientPerson classCode="PSN" determinerCode="INSTANCE"><name nullFlavor="NA"/></patientPerson></patient></subject1><custodian typeCode="CST"><assignedEntity classCode="ASSIGNED"><id root="{home}"/><code code="NotHealthDataLocator" codeSystem="1.3.6.1.4.1.19376.1.2.27.2"/></assignedEntity></custodian></registrationEvent></subject>"#
        );
    }
    text
}

/// A `PRPA_IN201306UV02` in its envelope, relating to `message`.
fn response(message: &str, ack: &str, query: &str, subjects: &str, reason: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope" xmlns:wsa="http://www.w3.org/2005/08/addressing"><env:Header><wsa:Action env:mustUnderstand="1">urn:hl7-org:v3:PRPA_IN201306UV02:CrossGatewayPatientDiscovery</wsa:Action><wsa:RelatesTo>{message}</wsa:RelatesTo></env:Header><env:Body><PRPA_IN201306UV02 xmlns="urn:hl7-org:v3" ITSVersion="XML_1.0"><id root="2.999.50.1.9" extension="1"/><creationTime value="20261003120000+0000"/><interactionId root="2.16.840.1.113883.1.6" extension="PRPA_IN201306UV02"/><processingCode code="P"/><processingModeCode code="T"/><acceptAckCode code="NE"/><receiver typeCode="RCV"><device classCode="DEV" determinerCode="INSTANCE"><id root="2.999.40.1"/></device></receiver><sender typeCode="SND"><device classCode="DEV" determinerCode="INSTANCE"><id root="2.999.50.1"/></device></sender><acknowledgement><typeCode code="{ack}"/></acknowledgement><controlActProcess classCode="CACT" moodCode="EVN"><code code="PRPA_TE201306UV02" codeSystem="2.16.840.1.113883.1.6"/>{subjects}{reason}<queryAck><queryId root="2.999.40.1.9" extension="1"/><queryResponseCode code="{query}"/></queryAck></controlActProcess></PRPA_IN201306UV02></env:Body></env:Envelope>"#
    )
}

/// A SOAP 1.2 `Receiver` fault relating to `message`.
fn fault(message: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope" xmlns:wsa="http://www.w3.org/2005/08/addressing"><env:Header><wsa:RelatesTo>{message}</wsa:RelatesTo></env:Header><env:Body><env:Fault><env:Code><env:Value>env:Receiver</env:Value></env:Code><env:Reason><env:Text xml:lang="en">synthetic outage</env:Text></env:Reason></env:Fault></env:Body></env:Envelope>"#
    )
}
