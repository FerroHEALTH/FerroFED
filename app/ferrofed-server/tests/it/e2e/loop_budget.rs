// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The per-run budget of the background loops (#2019): the allocations and
//! the store calls of one iteration of each loop, held to the ceilings
//! [`BUDGETS`] names, measured as the hot-path budget measures a request
//! ([`super::budget`]).
//!
//! The loops are the store's sweep of expired rows, over a real PostgreSQL
//! 19, so the case runs only when `FERROFED_E2E=1` admits the harness; the
//! registry reload of a registry document, as `SIGHUP` asks it; the PMIR
//! subscription's check against a stub Patient Identity Registry; and the mCSD directory's refresh against a stub Care
//! Services Directory, each in the steady state its loop runs in once warm.
//! Of [`MEASURED`] runs after [`WARM`] unmeasured ones, the least
//! allocations and the most store calls are judged. No specification governs
//! the cost of a loop: our own design.

use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use ferrofed_operator_api::health::Observed;
use ferrofed_server::binding::ihe::mcsd::registry::RefreshOutcome;
use ferrofed_server::config::Config;
use ferrofed_server::reload::Reloader;
use ferrofed_server::state::AppState;
use ferrofed_server::store::Store;
use ferrofed_testkit::containers::{self, database};
use ferrofed_testkit::mcsd::HarnessDirectory;
use ferrofed_testkit::pmir::PatientIdentityRegistry;

use super::TestResult;
use super::budget::{Cost, MEASURED, StoreCalls, WARM, counting, judge, warm};
use super::store::store_settings;
use crate::facade::{EHR_A, EHR_B, crossref, node_answering, registry};
use crate::pmir::{gateway, text};
use crate::registry_mcsd::{Gateway, members};

/// A background loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Loop {
    /// `store::sweeping`: one `Store::sweep`.
    Sweep,
    /// `reload::on_hangup`: one `Reloader::reload`.
    Reload,
    /// The PMIR subscription loop: one `IdentityFeed::check`.
    Pmir,
    /// The mCSD directory loop: one `DirectoryRegistry::refresh`.
    Mcsd,
}

/// The most one run of a loop may cost.
struct Budget {
    run: Loop,
    /// The allocations of the least measured run.
    allocations: u64,
    /// The calls one run makes into the store.
    store_calls: u64,
}

// NOTE: no specification governs these ceilings: our own design (#2019).
const BUDGETS: [Budget; 4] = [
    Budget {
        run: Loop::Sweep,
        allocations: 0,
        store_calls: 0,
    },
    Budget {
        run: Loop::Reload,
        allocations: 0,
        store_calls: 0,
    },
    Budget {
        run: Loop::Pmir,
        allocations: 0,
        store_calls: 0,
    },
    Budget {
        run: Loop::Mcsd,
        allocations: 0,
        store_calls: 0,
    },
];

/// A reloader over a registry document of the members at `a` and `b` and
/// the configuration file naming it, both written into `dir`, as `serve`
/// builds one.
fn reloading(dir: &Path, (a, b): (&str, &str)) -> Result<Reloader, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    let config = dir.join("ferrofed.toml");
    std::fs::write(&document, registry(a, b, ""))?;
    let named = toml::Value::String(document.display().to_string());
    std::fs::write(
        &config,
        crate::support::signed(&format!(
            "profile = \"development\"\n\n[registry]\ndocument = {named}\n\n[federation]\n\
             per_node_timeout = '2s'\noverall_timeout = '3s'\nnode_selection = \"ask-all\"\n\
             id = \"example-federation\"\n\n{}",
            crossref(&[("node-a", EHR_A), ("node-b", EHR_B)])
        )),
    )?;
    let settings = Config::resolve_loaded(&Config::load(&crate::support::file_sources(&config))?)?;
    let state = Arc::new(AppState::build(&settings)?);
    Ok(Reloader::new(Some(config), settings, state))
}

/// Runs `once` [`WARM`] and [`MEASURED`] times and returns the warm cost.
async fn runs<F, Fut>(calls: &StoreCalls, once: F) -> Result<Cost, Box<dyn Error>>
where
    F: Fn() -> Fut,
    Fut: Future<Output = TestResult>,
{
    let mut seen = Vec::new();
    for _ in 0..WARM + MEASURED {
        let (ran, cost) = counting(calls, once()).await;
        ran?;
        seen.push(cost);
    }
    warm(&seen)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_background_loop_holds_its_allocation_and_store_call_budget() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("budget_loops", &[]).await?;
    let dir = tempfile::tempdir()?;
    let settings = store_settings(&server, "budget_loops", dir.path())?;
    let store = tokio::task::block_in_place(|| Store::open(&settings))?;

    let (a, b) = (
        node_answering("synthetic-a").await,
        node_answering("synthetic-b").await,
    );
    let directory = HarnessDirectory::start().await;
    directory.publish(&members(&a.uri(), &b.uri()))?;
    let mcsd = tokio::task::block_in_place(|| Gateway::boot(&directory, 5_000))?;
    let reload_dir = tempfile::tempdir()?;
    let reloader =
        tokio::task::block_in_place(|| reloading(reload_dir.path(), (&a.uri(), &b.uri())))?;

    let registry = PatientIdentityRegistry::start().await?;
    let pmir_dir = tempfile::tempdir()?;
    let pmir = tokio::task::block_in_place(|| {
        gateway(&text(
            pmir_dir.path(),
            &registry.base_url(),
            "http://127.0.0.1:9/pmir/feed",
            "",
        )?)
    })?;
    let feed = pmir.state.identity_feed().ok_or("[pmir] is set")?;

    let calls = StoreCalls::installed()?;
    let mut rows = Vec::new();
    for budget in &BUDGETS {
        let cost = match budget.run {
            Loop::Sweep => {
                runs(&calls, || async {
                    store.sweep().await?;
                    Ok(())
                })
                .await?
            }
            Loop::Reload => {
                runs(&calls, || async {
                    tokio::task::block_in_place(|| reloader.reload())?;
                    Ok(())
                })
                .await?
            }
            Loop::Pmir => {
                runs(&calls, || async {
                    let observed = feed.check().await;
                    assert_eq!(
                        Observed::Up,
                        observed,
                        "the Registry holds the subscription"
                    );
                    Ok(())
                })
                .await?
            }
            Loop::Mcsd => {
                runs(&calls, || async {
                    let outcome = mcsd.directory.refresh(&mcsd.reloader).await;
                    assert!(
                        matches!(outcome, RefreshOutcome::Unchanged),
                        "the directory answers with no change: {outcome:?}"
                    );
                    Ok(())
                })
                .await?
            }
        };
        let ceiling = Cost {
            allocations: budget.allocations,
            store_calls: budget.store_calls,
        };
        rows.push((format!("{:?}", budget.run), cost, ceiling));
    }
    judge(&rows)
}
