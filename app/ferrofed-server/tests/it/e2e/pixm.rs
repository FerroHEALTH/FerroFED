// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The federated query over two FerroEHR nodes, the patient resolved through
//! FerroPIX, the family's PIX Manager, over ITI-83: only a member that knows
//! the patient is asked, by its own `ehr_id` (§11.1; N7, N8, N33).

use std::error::Error;

use ferrofed_testkit::containers;
use ferrofed_testkit::containers::ferropix::{self, CONSUMER, FerroPix, SOURCE};
use ferrofed_testkit::proxy::CapturingProxy;
use ferrofed_testkit::seed::{self, CrossReferenceSeed, DemoComposition, EhrDomain, PatientId};
use http::StatusCode;
use uuid::Uuid;

use crate::e2e::{
    Answer, EHR_A, EHR_B, PATIENT, TestResult, assert_no_patient_identifier_on_the_wire,
    body_holds, gateway_resolving, patient_query, plan, query,
};
use crate::support::call;

/// The `ehr_id` domains of node A and node B at the PIX Manager.
pub(crate) const DOMAIN_A: EhrDomain = EhrDomain::new(1);
pub(crate) const DOMAIN_B: EhrDomain = EhrDomain::new(2);

/// The unrelated patient fed beside [`PATIENT`], so both domains are known.
const OTHER: PatientId = PatientId::new(1, 39);

/// FerroPIX with a capturing proxy in front of it, which counts the ITI-83
/// calls the gateway makes.
pub(crate) struct Pix {
    /// FerroPIX and its database.
    manager: FerroPix,
    /// The proxy the gateway reaches FerroPIX through.
    proxy: CapturingProxy,
}

impl Pix {
    /// Returns the FHIR base the gateway asks, through the proxy.
    pub(crate) fn base_url(&self) -> String {
        format!("{}/fhir/", self.proxy.origin())
    }

    /// Returns FerroPIX's own origin, for a scenario that puts a proxy of its
    /// own in front of it.
    pub(crate) fn origin(&self) -> &str {
        self.manager.origin()
    }

    /// Returns how many ITI-83 calls reached FerroPIX through the proxy.
    pub(crate) fn queries(&self) -> usize {
        self.proxy
            .journal()
            .iter()
            .filter(|capture| capture.path.ends_with("/Patient/$ihe-pix"))
            .count()
    }
}

/// The two nodes, seeded, and FerroPIX fed over ITI-104 with the patient at
/// `ehrs`, plus an unrelated patient so both domains are known.
pub(crate) async fn nodes_and_pix(
    ehrs: Vec<(EhrDomain, Uuid)>,
) -> Result<(containers::TwoNodes, Pix), Box<dyn Error>> {
    let nodes = containers::two_nodes().await?;
    seed::seed(
        &nodes.a.api_root(),
        &plan(EHR_A, DemoComposition::FirstHospital),
    )
    .await?;
    seed::seed(
        &nodes.b.api_root(),
        &plan(EHR_B, DemoComposition::FirstClinic),
    )
    .await?;
    nodes.a.proxy.clear_journal();
    nodes.b.proxy.clear_journal();
    let mut domains = vec![PATIENT.namespace(), DOMAIN_A.system(), DOMAIN_B.system()];
    if OTHER.namespace() != PATIENT.namespace() {
        domains.push(OTHER.namespace());
    }
    let manager = ferropix::ferropix(&domains).await?;
    let fed = [
        CrossReferenceSeed {
            patient: PATIENT,
            ehrs,
        },
        CrossReferenceSeed {
            patient: OTHER,
            ehrs: vec![
                (
                    DOMAIN_A,
                    Uuid::from_u128(0x5555_5555_5555_4555_8555_5555_5555_5555),
                ),
                (
                    DOMAIN_B,
                    Uuid::from_u128(0x6666_6666_6666_4666_8666_6666_6666_6666),
                ),
            ],
        },
    ];
    for cross_reference in &fed {
        let status = seed::feed_as(
            &manager.fhir_base(),
            Some((SOURCE.user, SOURCE.password)),
            cross_reference,
        )
        .await?;
        assert_eq!(StatusCode::CREATED, status, "ITI-104 creates the Patient");
    }
    let proxy = CapturingProxy::start(manager.origin()).await?;
    Ok((nodes, Pix { manager, proxy }))
}

/// The resolver configuration over FerroPIX, through its proxy, under the
/// development profile, the only one that admits its plain `http`.
pub(crate) fn pixm_resolver(pix: &Pix) -> String {
    pixm_resolver_at(&pix.base_url())
}

/// The resolver configuration of [`pixm_resolver`], the Manager reached at
/// `url`, for a scenario that puts a capturing proxy of its own in front of
/// it. The gateway asks as FerroPIX's PIXm Consumer.
pub(crate) fn pixm_resolver_at(url: &str) -> String {
    format!(
        "profile = \"development\"\n\n[[pixm.manager]]\nurl = \"{url}\"\n\n[pixm.manager.members]\n\"node-a\" = \"{}\"\n\"node-b\" = \"{}\"\n\n[pixm.manager.credentials]\nuser = \"{}\"\npassword = \"{}\"\n",
        DOMAIN_A.system(),
        DOMAIN_B.system(),
        CONSUMER.user,
        CONSUMER.password
    )
}

// conformance: CP-3 CP-36 track-2
#[tokio::test]
async fn a_pix_resolved_query_asks_only_the_member_that_knows_the_patient() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let (nodes, pix) = Box::pin(nodes_and_pix(vec![(DOMAIN_A, EHR_A)])).await?;
    let resolver = pixm_resolver(&pix);
    let dir = tempfile::tempdir()?;

    let patient = patient_query();
    let app = gateway_resolving(dir.path(), &nodes.a, &nodes.b, &resolver)?;
    let (status, text) = call(app, query(&patient)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    crate::facade::schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert!(
        !answer.meta.federation.complete,
        "a not-resolved member leaves the answer incomplete (§11.1)"
    );
    let reported: Vec<(&str, &str, Option<u64>)> = answer
        .meta
        .federation
        .endpoints
        .iter()
        .map(|e| (e.id.as_str(), e.status.as_str(), e.row_count))
        .collect();
    assert_eq!(
        vec![
            ("node-a-pub", "active", Some(1)),
            ("node-b-pub", "not-resolved", None),
        ],
        reported,
        "{text}"
    );
    assert_eq!(1, answer.rows.len(), "the rows of node A alone: {text}");

    let journal = nodes.a.proxy.journal();
    let sent = journal.first().ok_or("node A was asked")?;
    assert!(
        body_holds(sent, &EHR_A.to_string()),
        "node A is asked by its own ehr_id (N7)"
    );
    assert!(
        nodes.b.proxy.journal().is_empty(),
        "node B, where the patient is not known, is never asked (N8)"
    );
    assert_no_patient_identifier_on_the_wire(&nodes);
    assert_eq!(1, pix.queries(), "one ITI-83 call for both members");
    Ok(())
}

// conformance: CP-3 track-2
#[tokio::test]
async fn a_patient_fed_at_both_members_resolves_through_pix_at_both() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let (nodes, pix) = Box::pin(nodes_and_pix(vec![(DOMAIN_A, EHR_A), (DOMAIN_B, EHR_B)])).await?;
    let resolver = pixm_resolver(&pix);
    let dir = tempfile::tempdir()?;

    let app = gateway_resolving(dir.path(), &nodes.a, &nodes.b, &resolver)?;
    let (status, text) = call(app, query(&patient_query())?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    crate::facade::schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert!(
        answer.meta.federation.complete,
        "both members resolved: {text}"
    );
    let reported: Vec<(&str, &str, Option<u64>)> = answer
        .meta
        .federation
        .endpoints
        .iter()
        .map(|e| (e.id.as_str(), e.status.as_str(), e.row_count))
        .collect();
    assert_eq!(
        vec![
            ("node-a-pub", "active", Some(1)),
            ("node-b-pub", "active", Some(1)),
        ],
        reported,
        "{text}"
    );
    for (node, own, other) in [(&nodes.a, EHR_A, EHR_B), (&nodes.b, EHR_B, EHR_A)] {
        let journal = node.proxy.journal();
        let sent = journal.first().ok_or("each node was asked")?;
        assert!(
            body_holds(sent, &own.to_string()) && !body_holds(sent, &other.to_string()),
            "{} is asked by its own ehr_id alone (N7)",
            node.node.system_id()
        );
    }
    assert_no_patient_identifier_on_the_wire(&nodes);
    assert_eq!(1, pix.queries(), "one ITI-83 call for both members");
    Ok(())
}
