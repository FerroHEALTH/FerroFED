// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The file, the environment over it, and the value each setting takes when unset.

use ferrofed_registry::id::EndpointId;
use ferrofed_server::config::error::Error;
use ferrofed_server::config::error::server::ServerFault;
use ferrofed_server::config::settings::Scheme;
use ferrofed_server::config::{COMBINING_MARGIN_MS, Config};
use ferrohealth_telemetry::section::Format;
use openehr_federation::aql::OffsetStrategy;
use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::num::NonZeroU32;
use std::time::Duration;

use super::{env, everything, federating, full, refusal};

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_file_states_every_section_and_the_resolver_reads_it() -> Result<(), Box<dyn StdError>> {
    let full = full();
    let settings = Config::from_sources(Some(full.as_str()), &BTreeMap::new())?.resolve()?;
    assert_eq!("0.0.0.0:9000", settings.server.listen.to_string());
    assert_eq!(Duration::from_millis(1000), settings.server.request_timeout);
    assert_eq!(
        Duration::from_millis(2000),
        settings.server.shutdown_timeout
    );
    assert_eq!(4096, settings.server.body_limit);
    assert_eq!(Format::Json, settings.telemetry.format);
    assert_eq!("debug", settings.telemetry.filter.as_str());
    match settings.credentials.get(&EndpointId::new("hospital-a")?) {
        Some(Scheme::Oauth { grant, .. }) => assert_eq!("ferrofed", grant.client_id),
        other => return Err(format!("hospital-a is an oauth scheme: {other:?}").into()),
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
    let full = full();
    let settings = Config::from_sources(
        Some(full.as_str()),
        &env("FERROFED__SERVER__LISTEN", "127.0.0.1:7777"),
    )?
    .resolve()?;
    assert_eq!("127.0.0.1:7777", settings.server.listen.to_string());
    let settings = Config::from_sources(
        Some(full.as_str()),
        &env("FERROFED__SERVER__BODY_LIMIT", "512B"),
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
    let full = full();
    let settings = Config::from_sources(Some(full.as_str()), &BTreeMap::new())?.resolve()?;
    assert!(
        settings.federation.best_effort,
        "offered, and opt-in per request"
    );
    let withdrawn = format!("{full}\n[federation]\nbest_effort = false\n");
    let settings = Config::from_sources(Some(&withdrawn), &BTreeMap::new())?.resolve()?;
    assert!(!settings.federation.best_effort);
    let settings = Config::from_sources(
        Some(full.as_str()),
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
    let full = full();
    let settings = Config::from_sources(Some(full.as_str()), &BTreeMap::new())?.resolve()?;
    assert_eq!(
        settings.federation.offset,
        OffsetStrategy::Bounded {
            max_window: NonZeroU32::new(1000).ok_or("1000 is not zero")?
        },
        "§11.6.2 option 2, with its bound"
    );
    let narrowed = format!("{full}\n[federation]\nmax_offset_window = 50\n");
    let settings = Config::from_sources(Some(&narrowed), &BTreeMap::new())?.resolve()?;
    assert_eq!(
        settings.federation.offset.max_window().map(NonZeroU32::get),
        Some(50)
    );
    let settings = Config::from_sources(
        Some(full.as_str()),
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
    let full = full();
    let names = |settings: &ferrofed_server::config::settings::Settings| -> Vec<&'static str> {
        settings
            .federation
            .decomposable
            .iter()
            .map(|function| function.name())
            .collect()
    };
    let settings = Config::from_sources(Some(full.as_str()), &BTreeMap::new())?.resolve()?;
    assert_eq!(
        names(&settings),
        ["COUNT", "SUM", "MIN", "MAX", "AVG"],
        "§11.6.3"
    );
    let narrowed =
        format!("{full}\n[federation]\ndecomposable_aggregates = [\"MAX\", \"COUNT\"]\n");
    let settings = Config::from_sources(Some(&narrowed), &BTreeMap::new())?.resolve()?;
    assert_eq!(names(&settings), ["COUNT", "MAX"], "in declaration order");
    let none = format!("{full}\n[federation]\ndecomposable_aggregates = []\n");
    let settings = Config::from_sources(Some(&none), &BTreeMap::new())?.resolve()?;
    assert!(names(&settings).is_empty(), "an empty list declares none");
    let unknown = refusal(&format!(
        "{full}\n[federation]\ndecomposable_aggregates = [\"MEDIAN\"]\n"
    ))?;
    assert!(matches!(unknown, Error::Load { .. }), "{unknown:?}");
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_zero_offset_window_or_an_unknown_strategy_refuses_to_boot() -> Result<(), Box<dyn StdError>> {
    let full = full();
    let zero = refusal(&format!("{full}\n[federation]\nmax_offset_window = 0\n"))?;
    assert!(
        matches!(&zero, Error::Zero { key } if key == "federation.max_offset_window"),
        "{zero:?}"
    );
    let unknown = refusal(&format!(
        "{full}\n[federation]\noffset_strategy = \"cursor\"\n"
    ))?;
    assert!(matches!(unknown, Error::Load { .. }), "{unknown:?}");
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
                Error::Server(ServerFault::Budget {
                    overall_ms: 25_000,
                    margin_ms: COMBINING_MARGIN_MS,
                    request_ms: refused,
                }) if refused == request_ms
            ),
            "{error:?}"
        );
        let message = error.to_string();
        assert!(
            message.contains("server.request_timeout")
                && message.contains("federation.overall_timeout"),
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
        Some(&format!(
            "[registry]\ndocument = \"/nonexistent/registry.toml\"\n{}",
            crate::support::audit_toml()
        )),
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

/// The keys `[server]` gave up to `[gateway]` and to the shared section are
/// read under their new names only: each old one is an unknown key, refused
/// with its name (#1490).
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_key_from_the_old_server_table_is_refused_by_its_name() -> Result<(), Box<dyn StdError>> {
    for (line, key) in [
        ("public_url = \"https://gateway.example.org\"", "public_url"),
        ("replica = \"gateway-eu-1\"", "replica"),
        ("bindings_drain_timeout = '8s'", "bindings_drain_timeout"),
        ("overload_retry_after_s = 3", "overload_retry_after_s"),
    ] {
        let error = refusal(&format!("[server]\n{line}\n"))?;
        let shown = everything(&error);
        assert!(
            shown.contains(key),
            "server.{key} is refused by its name: {shown}"
        );
    }
    Ok(())
}

/// The `[credentials."<endpoint id>".credentials.onward]` table of `endpoint`, with
/// `extra` written into it.
fn onward_lane(endpoint: &str, extra: &str) -> String {
    format!(
        "[credentials.\"{endpoint}\".credentials.oauth]\n\
         token_endpoint = \"https://as.example.org/token\"\n\
         client_id = \"ferrofed\"\n\
         client_auth = \"private_key_jwt\"\n\
         private_key_file = {key}\n\n\
         [credentials.\"{endpoint}\".credentials.onward]\n\
         issuer = \"https://federation.cdr-a.example.org\"\n\
         kind = \"persons\"\n\
         profile = \"rfc9068\"\n\
         audiences = [\"https://cdr-a.example.org/openehr\"]\n\
         indicators = [\"https://cdr-a.example.org/openehr\"]\n\
         {extra}\n\
         [credentials.\"{endpoint}\".credentials.onward.keys]\n\
         max_onward_lifetime = '1h'\n\n",
        key = toml::Value::String(crate::support::signing_key_file().to_owned())
    )
}

/// Whether the onward lane `settings` resolve for `endpoint` holds the
/// tokens it obtains.
fn caches(
    settings: &ferrofed_server::config::settings::Settings,
    endpoint: &str,
) -> Result<bool, Box<dyn StdError>> {
    match settings.credentials.get(&EndpointId::new(endpoint)?) {
        Some(Scheme::Oauth {
            onward: Some(onward),
            ..
        }) => Ok(onward.lane.onward.caches()),
        other => {
            Err(format!("{endpoint} is an oauth scheme with an onward lane: {other:?}").into())
        }
    }
}

/// An onward lane holds the tokens it obtains unless its table writes
/// `cache = false`, which a performance measurement's self-test sets so every
/// call is exchanged at the issuer (`ferrohealth-smart` 0.10 `OnwardSection`).
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn an_onward_lane_with_the_cache_off_holds_no_token() -> Result<(), Box<dyn StdError>> {
    let toml = format!(
        "[server]\nlisten = \"127.0.0.1:1\"\n\n{}{}",
        onward_lane("node-cached", ""),
        onward_lane("node-uncached", "cache = false\n"),
    );
    let settings = Config::from_sources(Some(&toml), &BTreeMap::new())?.resolve()?;
    assert!(
        caches(&settings, "node-cached")?,
        "the cache is on by default"
    );
    assert!(
        !caches(&settings, "node-uncached")?,
        "cache = false turns the lane's cache off"
    );
    Ok(())
}
