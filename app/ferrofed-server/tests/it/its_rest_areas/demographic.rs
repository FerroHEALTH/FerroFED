// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `its_rest.demographic` in every mode: `unsupported: 501` without a
//! DEMOGRAPHIC endpoint, and every request is `501` and asks nobody; routed
//! to the declared endpoint with one, and a request naming it or naming none
//! reaches it alone while one naming another is refused and asks nobody
//! (§7a.1, §12.6, N32).

use http::{Method, StatusCode};

use super::{ENDPOINT_A, ENDPOINT_B, Mode, Setup, TestResult, at, request, states};

/// A synthetic `OBJECT_VERSION_ID` of a PERSON node B holds.
const PARTY: &str = "6a1e7b1c-4d0f-4c3e-9a8e-5f2b1d0c9e7a::cdr-b.example.org::1";

// conformance: CP-23 CP-25
#[tokio::test]
async fn the_demographic_declaration_matches_the_demographic_area_in_each_mode() -> TestResult {
    let party = format!("/v1/demographic/person/{PARTY}");
    let read = |target| request(&Method::GET, &party, target, None);
    for mode in Mode::all() {
        let setup = Setup::new(mode, &[]).await?;
        let declared = setup.declared().await?.federation.its_rest.demographic;
        let declared = declared.as_str();
        if mode.demographic {
            states(declared, "routed-single-node:", mode);
            states(
                declared,
                "goes to the declared endpoint node-b-pub alone, never federated, whether or not the targeting headers name it",
                mode,
            );
            states(
                declared,
                "a header naming another endpoint is refused with targeting-conflict",
                mode,
            );
            // NOTE: §7a.1 DEMOGRAPHIC row, §12.6: the declared destination is the
            // explicit choice, so a request without a header goes there too.
            for target in [Some(ENDPOINT_B), None] {
                let routed = setup.send(read(target)?).await?;
                assert_eq!(
                    Some(ENDPOINT_B),
                    routed.acting.as_deref(),
                    "N31: {target:?}: {}",
                    routed.text
                );
                assert_eq!(
                    (Vec::new(), vec![at(&Method::GET, &party)]),
                    (routed.a, routed.b),
                    "N32: one node: {target:?}"
                );
            }
            for target in [Some(ENDPOINT_A), Some("*")] {
                let refused = setup.send(read(target)?).await?;
                assert_eq!(
                    StatusCode::BAD_REQUEST,
                    refused.status,
                    "§12.6: {target:?}: {}",
                    refused.text
                );
                assert!(refused.reached_nobody(), "{refused:?}");
            }
        } else {
            assert_eq!("unsupported: 501", declared, "N32: {mode:?}");
            for target in [None, Some(ENDPOINT_B)] {
                let refused = setup.send(read(target)?).await?;
                assert_eq!(
                    StatusCode::NOT_IMPLEMENTED,
                    refused.status,
                    "§7a.1: {target:?}: {}",
                    refused.text
                );
                assert_eq!("not-implemented", refused.code()?);
                assert!(refused.reached_nobody(), "{refused:?}");
            }
        }
    }
    Ok(())
}
