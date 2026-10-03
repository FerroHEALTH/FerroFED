// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `its_rest.ehr` in every mode: a new EHR, by `POST` or by `PUT` with an
//! `ehr_id`, goes only to the one endpoint the targeting headers name, and a
//! `PUT` of an `ehr_id` another member holds is refused; every other request
//! under a path `ehr_id` goes to the one node that owns it, found in the
//! order the declaration states; a versioned write goes only to the node
//! that controls the version; and the read by subject goes to the one member
//! that holds the subject (§5.2, §7a.1, §12.4, §12.5.1, N23, N33, N41).

use axum::body::Body;
use http::{Method, Request, StatusCode, header};

use super::{ENDPOINT_A, ENDPOINT_B, Mode, PROBED, Setup, TestResult, at, request, states};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT};

/// An `ehr_id` no member holds.
const FOREIGN: &str = "9999cccc-9999-4999-8999-999999999999";

/// A versioned object under [`PROBED`], whose versions a write amends.
const OBJECT: &str = "8849182c-82ad-4088-a07f-48ead4180515";

// conformance: CP-23
#[tokio::test]
async fn the_ehr_declaration_states_how_a_path_ehr_id_finds_its_owner() -> TestResult {
    let probe = at(&Method::GET, &format!("/v1/ehr/{PROBED}"));
    for mode in Mode::all() {
        let setup = Setup::new(mode, &[]).await?;
        let declared = setup.declared().await?.federation.its_rest.ehr;
        states(
            &declared,
            "every other request under {base}/v1/ehr/{ehr_id} goes to the one node that owns the ehr_id, found by the targeting headers, the ehr_id index, then for a read an ask-all probe",
            mode,
        );
        assert!(
            !declared.contains("binding"),
            "§12.5.1 step 2 applies to no request while there is no client session: {declared}"
        );

        states(&declared, "then for a read an ask-all probe", mode);
        let read = || request(&Method::GET, &format!("/v1/ehr/{PROBED}"), None, None);
        let probed = setup.send(read()?).await?;
        assert_eq!(StatusCode::OK, probed.status, "§12.5.1: {}", probed.text);
        assert_eq!(
            Some(ENDPOINT_A),
            probed.acting.as_deref(),
            "N31: the owner acted"
        );
        assert_eq!(
            (vec![probe.clone()], vec![probe.clone()]),
            (probed.a, probed.b),
            "§12.5.1 step 4: every member is probed"
        );

        states(&declared, "the ehr_id index", mode);
        let indexed = setup.send(read()?).await?;
        assert_eq!(StatusCode::OK, indexed.status, "{}", indexed.text);
        assert_eq!(
            (vec![probe.clone()], Vec::new()),
            (indexed.a, indexed.b),
            "§12.5.1 step 3: the index names the owner"
        );

        states(&declared, "found by the targeting headers", mode);
        let at_b = format!("/v1/ehr/{EHR_B}");
        let targeted = setup
            .send(request(&Method::GET, &at_b, Some(ENDPOINT_B), None)?)
            .await?;
        assert_eq!(
            Some(ENDPOINT_B),
            targeted.acting.as_deref(),
            "{}",
            targeted.text
        );
        assert_eq!(
            (Vec::new(), vec![at(&Method::GET, &at_b)]),
            (targeted.a, targeted.b),
            "§12.5.1 step 1"
        );

        let unrouted = setup
            .send(request(
                &Method::POST,
                &format!("/v1/ehr/{FOREIGN}/composition"),
                None,
                Some(("application/json", "{}".to_owned())),
            )?)
            .await?;
        assert_eq!(
            StatusCode::BAD_REQUEST,
            unrouted.status,
            "N41: a write is never probed for: {}",
            unrouted.text
        );
        assert_eq!("target-required", unrouted.code()?);
        assert!(unrouted.reached_nobody(), "{unrouted:?}");
    }
    Ok(())
}

// conformance: CP-23
#[tokio::test]
async fn the_ehr_declaration_states_that_a_versioned_write_needs_the_controlling_node() -> TestResult
{
    let amended = format!("/v1/ehr/{PROBED}/composition/{OBJECT}");
    let write = |system: &str| {
        Request::put(&amended)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::IF_MATCH, format!("\"{OBJECT}::{system}::1\""))
            .body(Body::from("{}"))
    };
    for mode in Mode::all() {
        let setup = Setup::new(mode, &[]).await?;
        let declared = setup.declared().await?.federation.its_rest.ehr;
        states(
            &declared,
            "a versioned write only when that node controls the version it amends",
            mode,
        );
        let owner = request(&Method::GET, &format!("/v1/ehr/{PROBED}"), None, None)?;
        let found = setup.send(owner).await?;
        assert_eq!(StatusCode::OK, found.status, "the index learns node A");
        let refused = setup.send(write("cdr-b.example.org")?).await?;
        assert_eq!(
            StatusCode::CONFLICT,
            refused.status,
            "§12.4, N23: {}",
            refused.text
        );
        assert!(refused.reached_nobody(), "{refused:?}");
        let controlled = setup.send(write("cdr-a.example.org")?).await?;
        assert_eq!(
            (vec![at(&Method::PUT, &amended)], Vec::new()),
            (controlled.a, controlled.b),
            "§12.4, N23"
        );
    }
    Ok(())
}

// conformance: CP-23
#[tokio::test]
async fn the_ehr_declaration_states_the_read_by_subject() -> TestResult {
    let by_subject = format!("/v1/ehr?subject_id={PATIENT}&subject_namespace={NAMESPACE}");
    for mode in Mode::all() {
        let setup = Setup::new(mode, &[("node-a", EHR_A)]).await?;
        let declared = setup.declared().await?.federation.its_rest.ehr;
        states(
            &declared,
            "GET {base}/v1/ehr?subject_id= resolves the subject and goes to the one member that holds it, by its ehr_id",
            mode,
        );
        let resolved = setup
            .send(request(&Method::GET, &by_subject, None, None)?)
            .await?;
        assert_eq!(
            (
                vec![at(&Method::GET, &format!("/v1/ehr/{EHR_A}"))],
                Vec::new()
            ),
            (resolved.a, resolved.b),
            "§5.2, N33"
        );
    }
    Ok(())
}

// conformance: CP-23 CP-15
#[tokio::test]
async fn the_ehr_declaration_states_that_a_new_ehr_goes_only_to_the_named_endpoint() -> TestResult {
    let post = |target| {
        request(
            &Method::POST,
            "/v1/ehr",
            target,
            Some(("application/json", "{}".to_owned())),
        )
    };
    let put =
        |ehr_id: &str, target| request(&Method::PUT, &format!("/v1/ehr/{ehr_id}"), target, None);
    for mode in Mode::all() {
        let setup = Setup::new(mode, &[]).await?;
        let declared = setup.declared().await?.federation.its_rest.ehr;
        states(
            &declared,
            "a new EHR, POST {base}/v1/ehr or PUT {base}/v1/ehr/{ehr_id}, goes only to the one endpoint the targeting headers name",
            mode,
        );
        let posted = setup.send(post(Some(ENDPOINT_B))?).await?;
        assert_eq!(
            (Vec::new(), vec![at(&Method::POST, "/v1/ehr")]),
            (posted.a, posted.b),
            "§12.4, N23"
        );
        let created = setup.send(put(FOREIGN, Some(ENDPOINT_B))?).await?;
        assert_eq!(
            (
                Vec::new(),
                vec![at(&Method::PUT, &format!("/v1/ehr/{FOREIGN}"))]
            ),
            (created.a, created.b),
            "§12.4, N23"
        );
        let found = setup
            .send(request(
                &Method::GET,
                &format!("/v1/ehr/{PROBED}"),
                None,
                None,
            )?)
            .await?;
        assert_eq!(StatusCode::OK, found.status, "the index learns node A");
        for untargeted in [post(None)?, put(PROBED, None)?] {
            let refused = setup.send(untargeted).await?;
            assert_eq!(
                StatusCode::BAD_REQUEST,
                refused.status,
                "§12.4: the index never routes a new EHR: {}",
                refused.text
            );
            assert_eq!("target-required", refused.code()?);
            assert!(refused.reached_nobody(), "{refused:?}");
        }

        states(
            &declared,
            "a PUT is refused when another member holds its ehr_id",
            mode,
        );
        let held = setup.send(put(PROBED, Some(ENDPOINT_B))?).await?;
        assert_eq!(StatusCode::CONFLICT, held.status, "§12.4: {}", held.text);
        assert_eq!("ehr-id-held", held.code()?);
        assert!(held.reached_nobody(), "{held:?}");
    }
    Ok(())
}
