// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! XCPD localization against FerroPIX's Responding Gateway: an undirected
//! patient query asks FerroPIX over ITI-55 which communities hold the
//! patient, and only the member serving the community it names is asked,
//! resolved by FerroPIX's PIXm Manager (ITI TF-2 §3.55; N4, N10, §14.1,
//! Annex A.3; CP-5, CP-11).
//!
//! FerroPIX answers for one community, `2.999.50`, which serves node A, and
//! holds the patient's record in node A's `ehr_id` domain alone, so node B is
//! `not-localized` and never asked. Every identifier is synthetic, inside the
//! `urn:oid:2.999` example arc.

use ferrofed_testkit::containers::{self, ferropix::Xcpd};

use crate::e2e::pixm::{DOMAIN_A, nodes_and_responding_pix, pixm_resolver};
use crate::e2e::scenario::{Options, asked, exchange, gateway_with, patient_predicate, post_aql};
use crate::e2e::track3::answered;
use crate::e2e::{EHR_A, TestResult, assert_no_patient_identifier_on_the_wire};

/// FerroPIX's community, which serves node A.
const COMMUNITY: &str = "2.999.50";

/// FerroPIX's Responding Gateway device.
const DEVICE: &str = "2.999.50.1";

/// The gateway's own community, the initiating community FerroPIX answers.
const HOME: &str = "2.999.40";

/// The undirected patient query.
fn undirected() -> String {
    format!(
        "SELECT c/uid/value AS uid FROM EHR e CONTAINS COMPOSITION c WHERE {}",
        patient_predicate()
    )
}

// conformance: CP-5 CP-11 track-3
#[tokio::test]
async fn an_undirected_query_asks_only_the_member_ferropix_names_over_iti_55() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let xcpd = Xcpd {
        home_community: COMMUNITY.to_owned(),
        device: DEVICE.to_owned(),
        domains: vec![DOMAIN_A.system()],
        initiating: vec![HOME.to_owned()],
    };
    let (nodes, pix) = Box::pin(nodes_and_responding_pix(
        vec![(DOMAIN_A, EHR_A)],
        Some(&xcpd),
    ))
    .await?;
    let dir = tempfile::tempdir()?;
    let options = Options {
        resolver: pixm_resolver(&pix),
        node_selection: "localized",
        tables: format!(
            "[federation.localization]\ntimeout_ms = 5000\n\n[xcpd]\nsender_device = \"2.999.40.1\"\n\
             home_community = \"{HOME}\"\naudit = \"log\"\n\n[[xcpd.gateway]]\nurl = \"{}\"\n\
             device = \"{DEVICE}\"\n\n[xcpd.communities]\n\"{COMMUNITY}\" = \"node-a\"\n",
            pix.xcpd_endpoint()
        ),
        ..Options::default()
    };
    let app = gateway_with(dir.path(), &nodes, &options)?;

    let reply = exchange(&app, post_aql(&undirected(), &[])?).await?;
    let answer = answered(&reply)?;
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "not-localized")],
        answer.statuses(),
        "CP-5, CP-11: the member no community FerroPIX names serves is not-localized: {}",
        reply.text
    );
    assert_eq!(1, answer.rows.len(), "{}", reply.text);
    assert!(asked(&nodes.b).is_empty(), "CP-5: node B is never asked");
    assert_no_patient_identifier_on_the_wire(&nodes);
    Ok(())
}
