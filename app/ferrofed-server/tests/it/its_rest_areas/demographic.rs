// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `its_rest.demographic` in every mode: `unsupported: 501` without a
//! DEMOGRAPHIC endpoint, and every request is `501` and asks nobody; routed
//! to the declared endpoint with one, and a request naming it reaches it
//! alone while any other is refused and asks nobody (§7a.1, §12.6, N32).

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
                "names the declared endpoint node-b-pub in the targeting headers and goes to it alone, never federated",
                mode,
            );
            let routed = setup.send(read(Some(ENDPOINT_B))?).await?;
            assert_eq!(
                Some(ENDPOINT_B),
                routed.acting.as_deref(),
                "N31: {}",
                routed.text
            );
            assert_eq!(
                (Vec::new(), vec![at(&Method::GET, &party)]),
                (routed.a, routed.b),
                "N32: one node"
            );
            for target in [None, Some(ENDPOINT_A), Some("*")] {
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
