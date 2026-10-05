// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The file, the environment over it, and the value each setting takes when unset.

use ferrofed_registry::id::EndpointId;
use ferrofed_server::config::error::Error;
use ferrofed_server::config::settings::Scheme;
use ferrofed_server::config::{COMBINING_MARGIN_MS, Config};
use ferrofed_server::telemetry::Format;
use openehr_federation::aql::OffsetStrategy;
use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::num::NonZeroU32;
use std::time::Duration;

use super::{FULL, env, federating, refusal};

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_file_states_every_section_and_the_resolver_reads_it() -> Result<(), Box<dyn StdError>> {
    let settings = Config::from_sources(Some(FULL), &BTreeMap::new())?.resolve()?;
    assert_eq!("0.0.0.0:9000", settings.server.listen.to_string());
    assert_eq!(Duration::from_millis(1000), settings.server.request_timeout);
    assert_eq!(
        Duration::from_millis(2000),
        settings.server.shutdown_timeout
    );
    assert_eq!(4096, settings.server.body_limit);
    assert_eq!(Format::Json, settings.telemetry.format);
    assert_eq!("debug", settings.telemetry.filter);
    match settings.credentials.get(&EndpointId::new("hospital-a")?) {
        Some(Scheme::Bearer(token)) => assert_eq!("synthetic-token", token.expose()),
        other => return Err(format!("hospital-a is a bearer scheme: {other:?}").into()),
    }
    match settings.credentials.get(&EndpointId::new("clinic-b")?) {
        Some(Scheme::Basic { user, password }) => {
            assert_eq!("gateway", user);
            assert_eq!("synthetic-password", password.expose());
        }
        other => return Err(format!("clinic-b is a basic scheme: {other:?}").into()),
    }
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn an_environment_override_wins_over_the_file_and_keeps_its_type() -> Result<(), Box<dyn StdError>>
{
    let settings = Config::from_sources(
        Some(FULL),
        &env("FERROFED__SERVER__LISTEN", "127.0.0.1:7777"),
    )?
    .resolve()?;
    assert_eq!("127.0.0.1:7777", settings.server.listen.to_string());
    let settings = Config::from_sources(
        Some(FULL),
        &env("FERROFED__SERVER__BODY_LIMIT_BYTES", "512"),
    )?
    .resolve()?;
    assert_eq!(512, settings.server.body_limit);
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn best_effort_is_offered_by_default_and_can_be_withdrawn() -> Result<(), Box<dyn StdError>> {
    let settings = Config::from_sources(Some(FULL), &BTreeMap::new())?.resolve()?;
    assert!(
        settings.federation.best_effort,
        "offered, and opt-in per request"
    );
    let withdrawn = format!("{FULL}\n[federation]\nbest_effort = false\n");
    let settings = Config::from_sources(Some(&withdrawn), &BTreeMap::new())?.resolve()?;
    assert!(!settings.federation.best_effort);
    let settings = Config::from_sources(
        Some(FULL),
        &env("FERROFED__FEDERATION__BEST_EFFORT", "false"),
    )?
    .resolve()?;
    assert!(!settings.federation.best_effort);
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn offset_paging_is_bounded_at_1000_rows_per_node_by_default() -> Result<(), Box<dyn StdError>> {
    let settings = Config::from_sources(Some(FULL), &BTreeMap::new())?.resolve()?;
    assert_eq!(
        settings.federation.offset,
        OffsetStrategy::Bounded {
            max_window: NonZeroU32::new(1000).ok_or("1000 is not zero")?
        },
        "§11.6.2 option 2, with its bound"
    );
    let narrowed = format!("{FULL}\n[federation]\nmax_offset_window = 50\n");
    let settings = Config::from_sources(Some(&narrowed), &BTreeMap::new())?.resolve()?;
    assert_eq!(
        settings.federation.offset.max_window().map(NonZeroU32::get),
        Some(50)
    );
    let settings = Config::from_sources(
        Some(FULL),
        &env("FERROFED__FEDERATION__OFFSET_STRATEGY", "reject"),
    )?
    .resolve()?;
    assert_eq!(settings.federation.offset, OffsetStrategy::Reject);
    assert_eq!(settings.federation.offset.name(), "reject");
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn every_aggregate_is_decomposable_by_default_and_the_list_can_narrow()
-> Result<(), Box<dyn StdError>> {
    let names = |settings: &ferrofed_server::config::settings::Settings| -> Vec<&'static str> {
        settings
            .federation
            .decomposable
            .iter()
            .map(|function| function.name())
            .collect()
    };
    let settings = Config::from_sources(Some(FULL), &BTreeMap::new())?.resolve()?;
    assert_eq!(
        names(&settings),
        ["COUNT", "SUM", "MIN", "MAX", "AVG"],
        "§11.6.3"
    );
    let narrowed =
        format!("{FULL}\n[federation]\ndecomposable_aggregates = [\"MAX\", \"COUNT\"]\n");
    let settings = Config::from_sources(Some(&narrowed), &BTreeMap::new())?.resolve()?;
    assert_eq!(names(&settings), ["COUNT", "MAX"], "in declaration order");
    let none = format!("{FULL}\n[federation]\ndecomposable_aggregates = []\n");
    let settings = Config::from_sources(Some(&none), &BTreeMap::new())?.resolve()?;
    assert!(names(&settings).is_empty(), "an empty list declares none");
    let unknown = refusal(&format!(
        "{FULL}\n[federation]\ndecomposable_aggregates = [\"MEDIAN\"]\n"
    ))?;
    assert!(matches!(unknown, Error::Parse { .. }), "{unknown:?}");
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_zero_offset_window_or_an_unknown_strategy_refuses_to_boot() -> Result<(), Box<dyn StdError>> {
    let zero = refusal(&format!("{FULL}\n[federation]\nmax_offset_window = 0\n"))?;
    assert!(
        matches!(&zero, Error::Zero { key } if key == "federation.max_offset_window"),
        "{zero:?}"
    );
    let unknown = refusal(&format!(
        "{FULL}\n[federation]\noffset_strategy = \"cursor\"\n"
    ))?;
    assert!(matches!(unknown, Error::Parse { .. }), "{unknown:?}");
    Ok(())
}

/// §11.5: a client can rely on an answer "within its declared overall budget,
/// plus combining time", so a request timeout at or under the budget plus
/// the combining margin is refused, naming both keys.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_request_timeout_not_past_the_budget_and_the_margin_refuses_to_boot()
-> Result<(), Box<dyn StdError>> {
    for request_ms in [20_000, 25_000, 25_000 + COMBINING_MARGIN_MS] {
        let error = refusal(&federating(request_ms, 25_000))?;
        assert!(
            matches!(
                error,
                Error::Budget {
                    overall_ms: 25_000,
                    margin_ms: COMBINING_MARGIN_MS,
                    request_ms: refused,
                } if refused == request_ms
            ),
            "{error:?}"
        );
        let message = error.to_string();
        assert!(
            message.contains("server.request_timeout_ms")
                && message.contains("federation.overall_timeout_ms"),
            "the refusal names both keys: {message}"
        );
    }
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_request_timeout_past_the_budget_and_the_margin_is_accepted() -> Result<(), Box<dyn StdError>> {
    let settings = Config::from_sources(
        Some(&federating(25_000 + COMBINING_MARGIN_MS + 1, 25_000)),
        &BTreeMap::new(),
    )?
    .resolve()?;
    assert_eq!(
        settings.federation.budget.overall(),
        Duration::from_secs(25)
    );
    let defaults = Config::from_sources(
        Some("[registry]\ndocument = \"/nonexistent/registry.toml\"\n"),
        &BTreeMap::new(),
    )?
    .resolve()?;
    assert!(
        defaults.server.request_timeout
            > defaults.federation.budget.overall() + Duration::from_millis(COMBINING_MARGIN_MS),
        "the defaults keep the relation"
    );
    Ok(())
}
