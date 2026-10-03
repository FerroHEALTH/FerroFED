// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `its_rest.definition` in every mode: a definition request goes to the one
//! endpoint the targeting headers name and nothing is picked for it; stored
//! queries are held at the gateway where the registry is offered and reach
//! no node; and a template upload naming `*` fans out only where the
//! template fan-out is offered. The `definition` object's booleans say the
//! same (§7a.1, §7a.2, §12.6, §12.7, N30, N43, N44).

use http::{Method, StatusCode};

use super::{ENDPOINT_A, ENDPOINT_B, Mode, NAME, Setup, TestResult, at, request, states, store};

/// The ADL 1.4 template collection (ITS-REST Definition API).
const ADL14: &str = "/v1/definition/template/adl1.4";

/// A synthetic operational template.
fn template() -> String {
    "<template xmlns=\"http://schemas.openehr.org/v1\">\n  <template_id><value>synthetic.declared.v1</value></template_id>\n</template>\n".to_owned()
}

// conformance: CP-23 CP-34
#[tokio::test]
async fn the_definition_declaration_states_that_a_request_goes_to_the_one_named_node() -> TestResult
{
    for mode in Mode::all() {
        let setup = Setup::new(mode, &[]).await?;
        let declared = setup.declared().await?.federation.its_rest.definition;
        let routed = if mode.registry {
            "a template request under {base}/v1/definition/template/ goes"
        } else {
            "a request under {base}/v1/definition/ goes"
        };
        states(&declared, routed, mode);
        states(
            &declared,
            "to the one endpoint the targeting headers name, never merged",
            mode,
        );
        let read = setup
            .send(request(&Method::GET, ADL14, Some(ENDPOINT_A), None)?)
            .await?;
        assert_eq!(
            (vec![at(&Method::GET, ADL14)], Vec::new()),
            (read.a, read.b),
            "§12.6: {mode:?}"
        );
        let unnamed = setup
            .send(request(&Method::GET, ADL14, None, None)?)
            .await?;
        assert_eq!(
            StatusCode::BAD_REQUEST,
            unnamed.status,
            "§12.6: no node is picked: {}",
            unnamed.text
        );
        assert!(unnamed.reached_nobody(), "{unnamed:?}");
    }
    Ok(())
}

// conformance: CP-23 CP-34
#[tokio::test]
async fn the_definition_declaration_states_where_stored_queries_are_held() -> TestResult {
    for mode in Mode::all() {
        let setup = Setup::new(mode, &[]).await?;
        let federation = setup.declared().await?.federation;
        let declared = federation.its_rest.definition;
        assert_eq!(
            mode.registry,
            declared.contains("stored queries at the gateway registry"),
            "§7a.2 definition-area-split: {mode:?}: {declared}"
        );
        assert_eq!(
            Some(mode.registry),
            federation.definition.stored_query_registry(),
            "N44"
        );
        assert_eq!(
            Some(false),
            federation.definition.stored_query_fan_out(),
            "N44: no definition reaches a node"
        );
        let held = setup.send(store(None)?).await?;
        if mode.registry {
            assert_eq!(StatusCode::OK, held.status, "§12.7: {}", held.text);
            assert!(
                held.reached_nobody(),
                "§12.7: the gateway holds it: {held:?}"
            );
        } else {
            assert_eq!(StatusCode::BAD_REQUEST, held.status, "§12.6: {}", held.text);
            assert_eq!("target-required", held.code()?);
            assert!(held.reached_nobody(), "{held:?}");
            let routed = setup.send(store(Some(ENDPOINT_A))?).await?;
            let path = format!("/v1/definition/query/{NAME}/1.0.0");
            assert_eq!(
                (vec![at(&Method::PUT, &path)], Vec::new()),
                (routed.a, routed.b),
                "§12.6"
            );
        }
    }
    Ok(())
}

// conformance: CP-23 CP-34
#[tokio::test]
async fn the_definition_declaration_states_the_template_fan_out_only_where_offered() -> TestResult {
    let upload = |target| {
        request(
            &Method::POST,
            ADL14,
            Some(target),
            Some(("application/xml", template())),
        )
    };
    for mode in Mode::all() {
        let setup = Setup::new(mode, &[]).await?;
        let federation = setup.declared().await?.federation;
        let declared = federation.its_rest.definition;
        assert_eq!(
            mode.fan_out,
            declared.contains(
                "a template upload naming * or several endpoints in the targeting headers fans out to each"
            ),
            "§7a.1, N43: {mode:?}: {declared}"
        );
        assert_eq!(
            mode.fan_out,
            federation.definition.fan_out_template_upload(),
            "N43"
        );
        let starred = setup.send(upload("*")?).await?;
        if mode.fan_out {
            let fanned = vec![at(&Method::POST, ADL14)];
            assert_eq!(
                (&fanned, &fanned),
                (&starred.a, &starred.b),
                "§12.6: each member: {}",
                starred.text
            );
        } else {
            assert_eq!(
                StatusCode::BAD_REQUEST,
                starred.status,
                "§12.6: {}",
                starred.text
            );
            assert!(starred.reached_nobody(), "{starred:?}");
        }
        let plain = setup.send(upload(ENDPOINT_B)?).await?;
        assert_eq!(
            (Vec::new(), vec![at(&Method::POST, ADL14)]),
            (plain.a, plain.b),
            "§12.6: one named node"
        );
    }
    Ok(())
}
