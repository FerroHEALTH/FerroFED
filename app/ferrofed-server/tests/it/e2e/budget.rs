// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The per-request budget of the hot paths (#2003): the allocations of the
//! thread that polls one request and the calls it makes into FerroFED's own
//! store, held to the ceilings [`BUDGETS`] names.
//!
//! The paths are the federated AQL query, the read of a stored query's
//! definition and the run of that stored query, over two `wiremock` members
//! with the stored-query registry on the `postgres` backend, and the
//! document list of the FHIR face over two members that serve their own
//! summaries, with its stable ids in the store; each store is a database on
//! a real PostgreSQL 19, so the case runs only when `FERROFED_E2E=1` admits
//! the harness. The allocations are counted by `allocation-counter`, the
//! global allocator of this test binary, on the polling thread alone; a
//! store call is a trace event of `CALL_TARGET` sent inside the request's
//! span. Of [`MEASURED`] requests after [`WARM`] unmeasured ones, the least
//! allocations and the most store calls are judged. The CPU instructions of
//! the in-process work are counted apart by callgrind
//! (`scripts/perf/instructions.sh`). No specification governs the cost of a
//! request: our own design.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context as TaskContext, Poll};

use axum::Router;
use axum::body::Body;
use ferrofed_server::config::Config;
use ferrofed_server::state::AppState;
use ferrofed_server::store::connection::CALL_TARGET;
use ferrofed_testkit::containers::{self, database};
use http::{Request, StatusCode, header};
use tracing::Instrument as _;
use tracing_subscriber::layer::{Context, SubscriberExt as _};
use tracing_subscriber::registry::LookupSpan;

use super::TestResult;
use super::store::store_toml;
use crate::facade::{
    EHR_A, EHR_B, NAMESPACE, PATIENT, body, crossref, node_answering, patient_query, post,
    registry, settings_with_room,
};
use crate::fhir::list::patient::SUMMARY_TYPE;
use crate::fhir::list::{DOCUMENT_A, DOCUMENT_B, Members, fhir_document, list};
use crate::support::call;

/// The requests of each path sent before the measured ones.
const WARM: usize = 2;

/// The measured requests of each path.
const MEASURED: usize = 5;

/// The span each measured request runs in.
const REQUEST_SPAN: &str = "hot_path_budget_request";

/// The stored query the stored-query paths read and run.
const NAME: &str = "org.example::budget_compositions";

/// A hot path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HotPath {
    /// `POST {base}/v1/query/aql` of a patient query.
    Query,
    /// `GET {base}/v1/definition/query/{name}/{version}`.
    Definition,
    /// `POST {base}/v1/query/{name}` with the patient as a parameter.
    Stored,
    /// `GET {fhir-base}/DocumentReference?patient.identifier=…`.
    List,
}

/// The most one request of a path may cost.
struct Budget {
    path: HotPath,
    /// The allocations of the least measured request.
    allocations: u64,
    /// The calls one request makes into the store.
    store_calls: u64,
}

// NOTE: no specification governs these ceilings: our own design (#2003).
const BUDGETS: [Budget; 4] = [
    Budget {
        path: HotPath::Query,
        allocations: 0,
        store_calls: 0,
    },
    Budget {
        path: HotPath::Definition,
        allocations: 0,
        store_calls: 0,
    },
    Budget {
        path: HotPath::Stored,
        allocations: 0,
        store_calls: 0,
    },
    Budget {
        path: HotPath::List,
        allocations: 0,
        store_calls: 0,
    },
];

/// What one request cost.
#[derive(Clone, Copy, Debug, Default)]
struct Cost {
    allocations: u64,
    store_calls: u64,
}

/// Counts the store-call events sent inside a [`REQUEST_SPAN`] span.
#[derive(Clone, Default)]
struct StoreCalls(Arc<AtomicU64>);

impl<S> tracing_subscriber::Layer<S> for StoreCalls
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        if event.metadata().target() != CALL_TARGET {
            return;
        }
        if ctx
            .event_scope(event)
            .is_some_and(|mut scope| scope.any(|span| span.name() == REQUEST_SPAN))
        {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// A future whose polls count their allocations into a shared cell.
struct Counted<F> {
    inner: Pin<Box<F>>,
    allocations: Rc<Cell<u64>>,
}

impl<F: Future> Future for Counted<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<F::Output> {
        let this = &mut *self;
        let mut out = Poll::Pending;
        let info = allocation_counter::measure(|| out = this.inner.as_mut().poll(cx));
        this.allocations
            .set(this.allocations.get().saturating_add(info.count_total));
        out
    }
}

/// Sends `request` to `app` in a [`REQUEST_SPAN`] span, refusing an answer
/// other than `200`, and returns what it cost.
async fn costed(
    calls: &StoreCalls,
    app: &Router,
    request: Request<Body>,
) -> Result<Cost, Box<dyn Error>> {
    let allocations = Rc::new(Cell::new(0));
    let before = calls.0.load(Ordering::Relaxed);
    let answer = Counted {
        inner: Box::pin(call(app.clone(), request).instrument(tracing::info_span!(REQUEST_SPAN))),
        allocations: Rc::clone(&allocations),
    }
    .await;
    let (status, text) = answer?;
    assert_eq!(StatusCode::OK, status, "{text}");
    Ok(Cost {
        allocations: allocations.get(),
        store_calls: calls.0.load(Ordering::Relaxed).saturating_sub(before),
    })
}

/// A gateway over the members at `a` and `b`, the patient resolved at both,
/// its stored-query registry on the `postgres` backend of the store `store`
/// names.
fn federated(dir: &Path, (a, b): (&str, &str), store: &str) -> Result<Router, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(&document, registry(a, b, ""))?;
    let document = toml::Value::String(document.display().to_string());
    let rows = crossref(&[("node-a", EHR_A), ("node-b", EHR_B)]);
    let text = format!(
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n\
         [federation]\nid = \"example-federation\"\nnode_selection = \"ask-all\"\n\
         per_node_timeout = '2s'\noverall_timeout = '3s'\n\n\
         [stored_queries]\nbackend = \"postgres\"\n{store}{rows}"
    );
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let state = Arc::new(AppState::build(&settings)?);
    state.lifecycle().booted();
    Ok(ferrofed_server::router(state, &settings_with_room()))
}

/// The stored query's definition: the patient by parameter, one column.
fn definition() -> String {
    format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = $patient \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    )
}

/// The request of `path`, the federation's or the face's.
fn request(path: HotPath) -> Result<Request<Body>, Box<dyn Error>> {
    Ok(match path {
        HotPath::Query => post(body(&patient_query())?)?,
        HotPath::Definition => {
            Request::get(format!("/v1/definition/query/{NAME}/1.0.0")).body(Body::empty())?
        }
        HotPath::Stored => Request::post(format!("/v1/query/{NAME}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(format!(
                r#"{{"query_parameters":{{"patient":"{PATIENT}"}}}}"#
            )))?,
        HotPath::List => list(SUMMARY_TYPE)?,
    })
}

/// Returns the least allocations and the most store calls of each path,
/// warm, each path sent to `app` of its gateway.
async fn measure(
    calls: &StoreCalls,
    (federation, face): (&Router, &Router),
) -> Result<Vec<(HotPath, Cost)>, Box<dyn Error>> {
    let mut costs = Vec::new();
    for path in BUDGETS.iter().map(|budget| budget.path) {
        let app = if path == HotPath::List {
            face
        } else {
            federation
        };
        let mut least: Option<Cost> = None;
        for attempt in 0..WARM + MEASURED {
            let cost = costed(calls, app, request(path)?).await?;
            if attempt < WARM {
                continue;
            }
            least = Some(match least {
                Some(seen) => Cost {
                    allocations: seen.allocations.min(cost.allocations),
                    store_calls: seen.store_calls.max(cost.store_calls),
                },
                None => cost,
            });
        }
        costs.push((path, least.ok_or("each path is measured")?));
    }
    Ok(costs)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_hot_path_holds_its_allocation_and_store_call_budget() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("budget_federation", &["budget_face"]).await?;
    let dir = tempfile::tempdir()?;
    let (a, b) = (
        node_answering("synthetic-a").await,
        node_answering("synthetic-b").await,
    );
    let store = store_toml(&server, "budget_federation", dir.path())?;
    let federation =
        tokio::task::block_in_place(|| federated(dir.path(), (&a.uri(), &b.uri()), &store))?;
    let put = Request::put(format!("/v1/definition/query/{NAME}/1.0.0"))
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from(definition()))?;
    let (status, text) = call(federation.clone(), put).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let face_store = store_toml(&server, "budget_face", dir.path())?;
    let face = tokio::task::block_in_place(|| members.gateway(dir.path(), ("", &face_store)))?;
    let calls = StoreCalls::default();
    let subscriber = tracing_subscriber::registry().with(calls.clone());
    let _default = tracing::subscriber::set_default(subscriber);
    let costs = measure(&calls, (&federation, &face)).await?;
    let mut report = String::from("path        allocations (budget)   store calls (budget)\n");
    let mut over = Vec::new();
    for (path, cost) in &costs {
        let budget = BUDGETS
            .iter()
            .find(|budget| budget.path == *path)
            .ok_or("each path has a budget")?;
        writeln!(
            report,
            "{path:<10?} {:>12} ({:>8}) {:>12} ({:>6})",
            cost.allocations, budget.allocations, cost.store_calls, budget.store_calls
        )?;
        if cost.allocations > budget.allocations || cost.store_calls > budget.store_calls {
            over.push(*path);
        }
    }
    println!("{report}");
    assert!(over.is_empty(), "over budget: {over:?}\n{report}");
    Ok(())
}
