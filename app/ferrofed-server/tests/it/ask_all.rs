// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ask-all node selection of a deployment with no localizer: every active
//! member's cross-reference is asked, and only a member that knows the patient
//! receives a query (§4.3 Variant B; N4 last sentence, N6, N8, N10).
//!
//! In process: three mock CDRs and the harness PIX Manager fed over ITI-104,
//! behind a capturing proxy, so the test reads what was asked of each.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::fmt::Write as _;

use ferrofed_testkit::pix::PixManager;
use ferrofed_testkit::proxy::CapturingProxy;
use ferrofed_testkit::seed::{self, CrossReferenceSeed, EhrDomain, PatientId};
use http::StatusCode;
use uuid::Uuid;

use crate::facade::{Answer, body, gateway, node_answering, post, received, statuses, wire};
use crate::support::call;

type TestResult = Result<(), Box<dyn Error>>;

/// The `ehr_id` domains of the three members at the PIX Manager.
const DOMAIN_A: EhrDomain = EhrDomain::new(1);
const DOMAIN_B: EhrDomain = EhrDomain::new(2);
const DOMAIN_C: EhrDomain = EhrDomain::new(3);

/// The patient's `ehr_id` at node A and node B; node C does not know them.
const EHR_A: Uuid = Uuid::from_u128(0x3a3a_3a3a_3a3a_4a3a_8a3a_3a3a_3a3a_3a3a);
const EHR_B: Uuid = Uuid::from_u128(0x3b3b_3b3b_3b3b_4b3b_8b3b_3b3b_3b3b_3b3b);

/// The patient the PIX Manager is fed with, in the example arc.
fn patient() -> PatientId {
    PatientId::new(1, 46)
}

/// The registry document of three members, one endpoint each.
fn registry(urls: [&str; 3]) -> String {
    let mut text = String::new();
    for (member, url) in ["a", "b", "c"].into_iter().zip(urls) {
        // NOTE: writing to a String cannot fail, so the result is dropped.
        let _written: std::fmt::Result = write!(
            text,
            "\n[[organisation]]\nid = \"org-{member}\"\n\n[[node]]\nid = \"node-{member}\"\norganisation = \"org-{member}\"\nsystem_id = \"cdr-{member}.example.org\"\n\n[[endpoint]]\nid = \"node-{member}-pub\"\nnode = \"node-{member}\"\nurl = \"{url}\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-{member}\"\n"
        );
    }
    text
}

/// The `[pixm]` resolver over the Manager at `base`, one domain per member.
fn pixm(base: &str) -> String {
    format!(
        "[[pixm.manager]]\nurl = \"{base}\"\n\n[pixm.manager.members]\n\"node-a\" = \"{}\"\n\"node-b\" = \"{}\"\n\"node-c\" = \"{}\"\n",
        DOMAIN_A.system(),
        DOMAIN_B.system(),
        DOMAIN_C.system()
    )
}

/// The undirected patient query, through `external_ref`.
fn query() -> String {
    let patient = patient();
    format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{}' \
         AND e/ehr_status/subject/external_ref/namespace = '{}'",
        patient.value(),
        patient.namespace()
    )
}

/// Whether the ITI-83 request named `domain` as a target system, written raw
/// or percent-encoded.
fn asked_for(proxy: &CapturingProxy, domain: EhrDomain) -> bool {
    let system = domain.system();
    proxy.journal_contains(system.as_bytes())
        || proxy.journal_contains(system.replace(':', "%3A").as_bytes())
}

// conformance: CP-3
#[tokio::test]
async fn an_undirected_patient_query_over_three_members_dispatches_to_the_two_that_resolve()
-> TestResult {
    let pix = PixManager::start().await?;
    let feeds = [
        CrossReferenceSeed {
            patient: patient(),
            ehrs: vec![(DOMAIN_A, EHR_A), (DOMAIN_B, EHR_B)],
        },
        // NOTE: an unrelated patient makes domain C known to the Manager, so
        // node C is a member that does not know this patient.
        CrossReferenceSeed {
            patient: PatientId::new(1, 47),
            ehrs: vec![(
                DOMAIN_C,
                Uuid::from_u128(0x3c3c_3c3c_3c3c_4c3c_8c3c_3c3c_3c3c_3c3c),
            )],
        },
    ];
    for feed in &feeds {
        assert_eq!(
            StatusCode::CREATED,
            seed::feed(&pix.base_url(), feed).await?,
            "ITI-104 creates the Patient"
        );
    }
    let proxy = CapturingProxy::start(pix.origin()).await?;
    let a = node_answering("a-uid::cdr-a.example.org::1").await;
    let b = node_answering("b-uid::cdr-b.example.org::1").await;
    let c = node_answering("c-uid::cdr-c.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let app = gateway(
        dir.path(),
        &registry([&a.uri(), &b.uri(), &c.uri()]),
        "profile = \"development\"",
        &pixm(&format!("{}/fhir/", proxy.origin())),
    )?;
    let (status, answer) = call(app, post(body(&query())?)?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "an unknown member fails nothing (N6): {answer}"
    );
    let answer: Answer = serde_json::from_str(&answer)?;

    assert_eq!(
        vec![
            ("node-a-pub", "active"),
            ("node-b-pub", "active"),
            ("node-c-pub", "not-resolved"),
        ],
        statuses(&answer),
        "every member is reported (§11.1)"
    );
    assert_eq!(
        2,
        answer.rows.len(),
        "one row from each member that resolved"
    );
    assert!(
        !answer.meta.federation.complete,
        "a not-resolved member clears complete without failing the query (§11.3)"
    );

    assert_eq!(1, received(&a).await?.len(), "node A is asked once");
    assert_eq!(1, received(&b).await?.len(), "node B is asked once");
    assert!(
        received(&c).await?.is_empty(),
        "node C does not know the patient and is never asked (N8)"
    );

    assert_eq!(1, pix.queries(), "one ITI-83 call covers every member");
    for domain in [DOMAIN_A, DOMAIN_B, DOMAIN_C] {
        assert!(
            asked_for(&proxy, domain),
            "every member's domain is a target of the cross-reference (§4.3, N4)"
        );
    }

    let identifier = patient().value();
    for (node, server) in [("A", &a), ("B", &b), ("C", &c)] {
        assert!(
            !wire(server).await?.contains(&identifier),
            "the patient identifier reached node {node} (N33)"
        );
    }
    Ok(())
}
