// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `its_rest.query` in every mode: the `GET` and `POST` forms of
//! `{base}/v1/query/aql` fan out, and so do the forms of
//! `{base}/v1/query/{name}[/{version}]` where the registry is offered; where
//! it is not, a stored query invoked by name is `501` and asks nobody (§7,
//! §7a.1, §12.7, N1, N30, N44).

use http::{Method, StatusCode};

use super::{Mode, NAME, Setup, TestResult, at, request, states, store};
use crate::facade::{EHR_A, EHR_B, PATIENT, body, patient_query};
use crate::query_get::encoded;

// conformance: CP-23
#[tokio::test]
async fn the_query_declaration_states_that_both_forms_of_an_adhoc_query_fan_out() -> TestResult {
    let fanned = vec![at(&Method::POST, "/v1/query/aql")];
    for mode in Mode::all() {
        let setup = Setup::new(mode, &[("node-a", EHR_A), ("node-b", EHR_B)]).await?;
        let declared = setup.declared().await?.federation.its_rest.query;
        states(
            &declared,
            "federated: GET and POST {base}/v1/query/aql",
            mode,
        );
        let posted = request(
            &Method::POST,
            "/v1/query/aql",
            None,
            Some(("application/json", body(&patient_query())?)),
        )?;
        let got = request(
            &Method::GET,
            &format!("/v1/query/aql?q={}", encoded(&patient_query())),
            None,
            None,
        )?;
        for sent in [posted, got] {
            let outcome = setup.send(sent).await?;
            assert_eq!(
                StatusCode::OK,
                outcome.status,
                "N1: {mode:?}: {}",
                outcome.text
            );
            assert_eq!(
                (&fanned, &fanned),
                (&outcome.a, &outcome.b),
                "§7: every member: {mode:?}"
            );
        }
    }
    Ok(())
}

// conformance: CP-23
#[tokio::test]
async fn the_query_declaration_names_stored_query_execution_only_where_the_registry_is_offered()
-> TestResult {
    let fanned = vec![at(&Method::POST, "/v1/query/aql")];
    for mode in Mode::all() {
        let setup = Setup::new(mode, &[("node-a", EHR_A), ("node-b", EHR_B)]).await?;
        let declared = setup.declared().await?.federation.its_rest.query;
        assert_eq!(
            mode.registry(),
            declared.contains("GET and POST {base}/v1/query/{name}[/{version}] fan out"),
            "§7a.1, §12.7: {mode:?}: {declared}"
        );
        if mode.registry() {
            let held = setup.send(store("1.0.0", None)?).await?;
            assert_eq!(StatusCode::OK, held.status, "§12.7: stored: {}", held.text);
        }
        let bound = format!(r#"{{"query_parameters":{{"patient":"{PATIENT}"}}}}"#);
        let invoked = [
            request(
                &Method::POST,
                &format!("/v1/query/{NAME}"),
                None,
                Some(("application/json", bound)),
            )?,
            request(
                &Method::GET,
                &format!("/v1/query/{NAME}/1.0.0?patient={PATIENT}"),
                None,
                None,
            )?,
        ];
        for invocation in invoked {
            let outcome = setup.send(invocation).await?;
            if mode.registry() {
                assert_eq!(
                    StatusCode::OK,
                    outcome.status,
                    "§12.7: {mode:?}: {}",
                    outcome.text
                );
                assert_eq!(
                    (&fanned, &fanned),
                    (&outcome.a, &outcome.b),
                    "§12.7: every member"
                );
            } else {
                assert_eq!(
                    StatusCode::NOT_IMPLEMENTED,
                    outcome.status,
                    "§12.6: {mode:?}: {}",
                    outcome.text
                );
                assert!(outcome.reached_nobody(), "{mode:?}: {outcome:?}");
            }
        }
    }
    Ok(())
}
