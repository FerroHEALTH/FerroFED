// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Learned state held to the new document: withdrawn routes and departed members.

use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use ferrofed_identity::binding::{Bound, ResolutionBindings, SessionKey};
use ferrofed_registry::creating_system::{CreatingSystemRoute, Sighting};
use ferrofed_registry::ehr_index::{EhrIndex, Indexed};
use ferrofed_registry::error::CreatingSystemMiss;
use ferrofed_registry::id::{EhrId, NodeId, SystemId};
use ferrofed_registry::incident::Incident;
use ferrofed_server::facade::owner::{self, Held, Located};
use ferrofed_server::telemetry::{Rendering, subscriber};
use http::{HeaderMap, StatusCode};
use openehr_base::prelude::ObjectVersionId;

use ferrofed_server::auth::caller::{Caller, Stated, VerifiedBy};

use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, crossref, node_answering};
use crate::path_ehr_id::{answer, probe_at};
use crate::support::{Logs, asked, claims, error_body};

use super::{
    Gateway, LEGACY, TestResult, VERSION_A, indexed_at_a_departed_member, member,
    snapshot_of_a_and_b,
};

#[tokio::test]
async fn a_reload_that_contradicts_a_learned_route_withdraws_it_with_its_incident() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-b::cdr-b.example.org::1").await;
    let members = member("a", &a.uri()) + &member("b", &b.uri());
    let gateway = Gateway::start(&members, "", &crossref(&[("node-a", EHR_A)]))?;
    let legacy: SystemId = LEGACY.parse()?;
    let version =
        ObjectVersionId::new(format!("8849182c-82ad-4088-a07f-48ead4180515::{LEGACY}::1"))?;
    let running = gateway.federation()?;
    let sighting =
        running
            .learned()
            .observe(running.snapshot(), &version, &"node-a-pub".parse()?)?;
    assert!(matches!(sighting, Sighting::Learned(_)), "{sighting:?}");

    let mapped = format!(
        "{members}\n[[creating_system]]\ncreating_system_id = \"{LEGACY}\"\nendpoint = \"node-b-pub\"\n"
    );
    gateway.write_registry(&mapped)?;
    let logs = Logs::default();
    let capture = subscriber(Rendering::Json, "info", false, logs.clone())?;
    let reloaded = tracing::subscriber::with_default(capture, || gateway.reloader.reload());
    let applied = reloaded?;

    assert_eq!(
        vec![Incident::RegisteredCreatingSystemConflict {
            creating_system_id: legacy.clone(),
            registered: "node-b".parse()?,
            learned: "node-a-pub".parse()?,
        }],
        applied.reconciled.incidents
    );
    let text = logs.text();
    assert_eq!(
        1,
        text.matches("\"RegisteredCreatingSystemConflict\"").count(),
        "the incident is emitted once: {text}"
    );
    let federation = gateway.federation()?;
    let route = federation.learned().route(federation.snapshot(), &legacy)?;
    assert!(
        matches!(&route, CreatingSystemRoute::Registered { .. }),
        "the document routes it now: {route:?}"
    );

    gateway.write_registry(&members)?;
    gateway.reloader.reload()?;
    let federation = gateway.federation()?;
    let route = federation.learned().route(federation.snapshot(), &legacy);
    assert_eq!(
        Err(CreatingSystemMiss::Conflicted(legacy)),
        route,
        "the learned route stays withdrawn when the mapping goes again"
    );
    Ok(())
}

#[tokio::test]
async fn a_departed_member_leaves_the_index_and_the_bindings() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-b::cdr-b.example.org::1").await;
    let tables = crossref(&[("node-a", EHR_A)]);
    let gateway = Gateway::start(
        &(member("a", &a.uri()) + &member("b", &b.uri())),
        "",
        &tables,
    )?;
    let (ehr_a, ehr_b): (EhrId, EhrId) = (EHR_A.parse()?, EHR_B.parse()?);
    let (node_a, node_b): (NodeId, NodeId) = ("node-a".parse()?, "node-b".parse()?);
    let session = SessionKey::new("session-reload");
    let now = Instant::now();
    let running = gateway.federation()?;
    running.index().learn(&ehr_a, &node_a);
    running.index().learn(&ehr_b, &node_b);
    running
        .bindings()
        .record(&session, now, [(&node_a, &ehr_a), (&node_b, &ehr_b)]);

    gateway.write_registry(&member("a", &a.uri()))?;
    let applied = gateway.reloader.reload()?;

    assert_eq!(
        (1, 1),
        (
            applied.reconciled.index_dropped,
            applied.reconciled.bindings_dropped
        )
    );
    let federation = gateway.federation()?;
    assert_eq!(Indexed::None, federation.index().lookup(&ehr_b));
    assert_eq!(
        Indexed::One(node_a.clone()),
        federation.index().lookup(&ehr_a)
    );
    assert_eq!(
        Bound::None,
        federation.bindings().lookup(&session, now, &ehr_b)
    );
    assert_eq!(
        Bound::One(node_a),
        federation.bindings().lookup(&session, now, &ehr_a)
    );
    Ok(())
}

#[test]
fn a_binding_naming_a_departed_claimant_is_dropped_and_never_narrowed() -> TestResult {
    let snapshot = snapshot_of_a_and_b()?;
    let ehr_a: EhrId = EHR_A.parse()?;
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let session = SessionKey::new("session-1");
    let now = Instant::now();
    bindings.record(
        &session,
        now,
        [
            (&"node-a".parse()?, &ehr_a),
            (&"node-gone".parse()?, &ehr_a),
        ],
    );
    let index = EhrIndex::new(NonZeroUsize::MIN);
    let held = Held {
        bindings: &bindings,
        session: &session,
        now,
    };
    let located = owner::located(&snapshot, &HeaderMap::new(), Some(held), &index, &ehr_a)?;
    assert!(
        matches!(located, Located::Unknown),
        "never narrowed to node-a (§12.5.2, N42): {located:?}"
    );
    assert_eq!(
        Bound::None,
        bindings.lookup(&session, now, &ehr_a),
        "the stale binding is dropped"
    );
    Ok(())
}

#[test]
fn an_index_entry_naming_a_departed_claimant_is_dropped_and_never_narrowed() -> TestResult {
    let snapshot = snapshot_of_a_and_b()?;
    let ehr_a: EhrId = EHR_A.parse()?;
    let index = EhrIndex::new(NonZeroUsize::MIN);
    index.learn(&ehr_a, &"node-a".parse()?);
    index.learn(&ehr_a, &"node-gone".parse()?);
    let located = owner::located(&snapshot, &HeaderMap::new(), None, &index, &ehr_a)?;
    assert!(
        matches!(located, Located::Unknown),
        "never narrowed to node-a (§12.5.2, N42): {located:?}"
    );
    assert_eq!(
        Indexed::None,
        index.lookup(&ehr_a),
        "the stale entry is dropped"
    );
    Ok(())
}

#[tokio::test]
async fn a_read_never_routes_on_an_entry_naming_a_departed_claimant() -> TestResult {
    let (gateway, a) = indexed_at_a_departed_member().await?;
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    let request = http::Request::get(&resource).body(axum::body::Body::empty())?;

    let (status, acting, text) = answer(gateway.app.clone(), request).await?;

    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(Some("node-a-pub"), acting.as_deref());
    assert_eq!(
        vec![probe_at(), ("GET".to_owned(), resource)],
        asked(&a).await?,
        "the stale entry is dropped and the ask-all probe finds the owner (§12.5.1, N42)"
    );
    Ok(())
}

#[tokio::test]
async fn a_write_never_routes_on_an_entry_naming_a_departed_claimant() -> TestResult {
    let (gateway, a) = indexed_at_a_departed_member().await?;
    let request = http::Request::post(format!("/v1/ehr/{EHR_A}/composition"))
        .body(axum::body::Body::from(r#"{"_type":"COMPOSITION"}"#))?;

    let (status, acting, text) = answer(gateway.app.clone(), request).await?;

    assert_eq!(
        (StatusCode::BAD_REQUEST, "target-required".to_owned()),
        (status, error_body(&text)?.code),
        "never sent to node-a on a narrowed entry (§12.5.2, N42): {text}"
    );
    assert!(acting.is_none());
    assert!(asked(&a).await?.is_empty(), "node A is sent nothing");
    assert_eq!(
        Indexed::None,
        gateway.federation()?.index().lookup(&EHR_A.parse()?),
        "the stale entry is dropped"
    );
    Ok(())
}

/// The session of the default test caller, as the gateway keys its
/// resolution bindings.
fn default_session() -> SessionKey {
    let claims = claims();
    Caller::new(
        Stated {
            issuer: claims.iss,
            subject: claims.sub,
            client_id: claims.client_id,
            organisation: None,
            granted: String::new(),
            purposes: Vec::new(),
        },
        VerifiedBy::Signature,
    )
    .session()
}

#[tokio::test]
async fn a_consent_denial_drops_the_callers_binding_and_no_signal_keeps_it() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-b::cdr-b.example.org::1").await;
    let members = member("a", &a.uri()) + &member("b", &b.uri());
    let rows = crossref(&[("node-a", EHR_A)]);
    let gateway = Gateway::start(&members, "", &rows)?;
    let (session, ehr_a) = (default_session(), EHR_A.parse()?);

    let (status, text) = gateway.ask().await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        Bound::One("node-a".parse()?),
        gateway
            .federation()?
            .bindings()
            .lookup(&session, Instant::now(), &ehr_a),
        "the caller's query binds the member that resolved (§12.5.1 step 2), with no consent signal at all"
    );

    let denied = format!(
        "{rows}\n[[dev.consent_denied]]\nnamespace = \"{NAMESPACE}\"\nvalue = \"{PATIENT}\"\nmember = \"node-a\"\n"
    );
    gateway.write_config("", &denied)?;
    gateway.reloader.reload()?;
    let (status, text) = gateway.ask().await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        Bound::None,
        gateway
            .federation()?
            .bindings()
            .lookup(&session, Instant::now(), &ehr_a),
        "a consent denial drops the binding naming the denied member (N27a)"
    );
    Ok(())
}
