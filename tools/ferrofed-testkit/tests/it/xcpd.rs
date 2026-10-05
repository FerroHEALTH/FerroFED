// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stub responding gateway, held to the XCPD client of `ihe-iti`: every
//! answer it gives reads as the ITI-55 case it stands for (ITI TF-2
//! §3.55.4.2.3).

use std::collections::BTreeMap;
use std::time::Duration;

use ferrofed_testkit::xcpd::{Answer, Community, RespondingGateway};
use ihe_iti::user::OnBehalfOf;
use ihe_iti::xcpd::XcpdClient;
use ihe_iti::xcpd::discovery::Discovery;
use ihe_iti::xcpd::error::XcpdError;
use ihe_iti::xcpd::identifier::{Oid, PatientIdentifier};
use ihe_iti::xcpd::request::{self, DiscoveryQuery};
use secrecy::SecretString;
use url::Url;

type TestResult = Result<(), Box<dyn std::error::Error>>;

async fn ask(
    stub: &RespondingGateway,
    patient: &str,
) -> Result<Result<Discovery, XcpdError>, Box<dyn std::error::Error>> {
    let gateway = request::RespondingGateway::unencrypted_for_development(
        Url::parse(&stub.endpoint())?,
        Oid::new("2.999.50.1")?,
    )?;
    let identifier = PatientIdentifier::new(Oid::new("2.999.1")?, SecretString::from(patient))?;
    let query = DiscoveryQuery::new(Oid::new("2.999.40.1")?, identifier);
    let client = XcpdClient::new(reqwest::Client::builder().build()?);
    Ok(client
        .discover(
            &gateway,
            &query,
            None,
            &OnBehalfOf::System,
            Duration::from_millis(500),
        )
        .await)
}

#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn each_answer_reads_as_its_case() -> TestResult {
    let answers = BTreeMap::from([
        (
            "SYNTH-A".to_owned(),
            Answer::Holds(vec![
                Community::new("2.999.50", "2.999.50.2", "PID-A-50"),
                Community::new("2.999.60", "2.999.60.2", "PID-A-60"),
            ]),
        ),
        ("SYNTH-BUSY".to_owned(), Answer::Busy),
        ("SYNTH-FAULT".to_owned(), Answer::Fault),
        ("SYNTH-SILENT".to_owned(), Answer::Silent),
    ]);
    let stub = RespondingGateway::start(answers, Answer::NoMatch).await;

    let held = ask(&stub, "SYNTH-A").await??;
    let communities: Vec<String> = held.communities().iter().map(ToString::to_string).collect();
    assert_eq!(vec!["urn:oid:2.999.50", "urn:oid:2.999.60"], communities);
    assert!(matches!(
        ask(&stub, "SYNTH-UNKNOWN").await?,
        Ok(Discovery::NoMatch)
    ));
    assert!(matches!(
        ask(&stub, "SYNTH-BUSY").await?,
        Err(XcpdError::ApplicationError { .. })
    ));
    assert!(matches!(
        ask(&stub, "SYNTH-FAULT").await?,
        Err(XcpdError::Fault { .. })
    ));
    assert!(matches!(
        ask(&stub, "SYNTH-SILENT").await?,
        Err(XcpdError::Timeout)
    ));
    assert_eq!(5, stub.requests().await.len(), "every request is recorded");
    Ok(())
}
