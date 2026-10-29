// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `gateway.replica`: the name every operator report carries, from the
//! configuration, from the host name the process runs with, or `unnamed`.
//! No specification governs it: our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error as StdError;

use ferrofed_registry::replica::{ReplicaError, UNNAMED};
use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error;
use ferrofed_server::config::error::server::ServerFault;
use ferrofed_server::config::gateway::Gateway;

type TestResult = Result<(), Box<dyn StdError>>;

/// The `[gateway]` section `text` writes.
fn server(text: &str) -> Result<Gateway, Box<dyn StdError>> {
    Ok(Config::from_sources(Some(text), &BTreeMap::new())?.gateway)
}

#[test]
fn a_configured_name_is_the_replica_whatever_the_host_is_called() -> TestResult {
    let section = server("[gateway]\nreplica = \"gateway-eu-1\"\n")?;
    let replica = section.replica_or_host(Some("ferrofed-7d9c4b5f6-x2k8q"))?;
    assert_eq!("gateway-eu-1", replica.as_str());
    Ok(())
}

#[test]
fn with_no_name_configured_the_host_name_is_the_replica() -> TestResult {
    let section = server("")?;
    let replica = section.replica_or_host(Some("ferrofed-7d9c4b5f6-x2k8q"))?;
    assert_eq!("ferrofed-7d9c4b5f6-x2k8q", replica.as_str());
    Ok(())
}

#[test]
fn with_no_name_and_no_usable_host_name_the_replica_is_unnamed() -> TestResult {
    let section = server("")?;
    assert_eq!(UNNAMED, section.replica_or_host(None)?.as_str());
    assert_eq!(
        UNNAMED,
        section.replica_or_host(Some("not a host name"))?.as_str()
    );
    assert_eq!(UNNAMED, section.replica_or_host(Some(""))?.as_str());
    Ok(())
}

#[test]
fn a_configured_name_outside_the_form_is_refused_at_load() -> TestResult {
    for (written, expected) in [
        ("pod a", ReplicaError::Character { found: ' ' }),
        ("", ReplicaError::Empty),
        (&"a".repeat(65), ReplicaError::TooLong { length: 65 }),
    ] {
        let text = format!("[gateway]\nreplica = {}\n", toml::Value::from(written));
        let refused = Config::from_sources(Some(&text), &BTreeMap::new())?
            .resolve()
            .err()
            .ok_or("a bad replica name is refused")?;
        assert!(
            matches!(&refused, Error::Server(ServerFault::Replica(reason)) if *reason == expected),
            "{written:?}: {refused:?}"
        );
    }
    Ok(())
}

#[test]
fn the_environment_names_the_replica_as_the_file_does() -> TestResult {
    let environment = BTreeMap::from([(
        String::from("FERROFED__GATEWAY__REPLICA"),
        String::from("gateway-eu-2"),
    )]);
    let settings = Config::from_sources(None, &environment)?.resolve()?;
    assert_eq!("gateway-eu-2", settings.server.replica.as_str());
    Ok(())
}
