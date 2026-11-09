// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ferrofed-perf-token`: the performance gate's token endpoint
//! ([`ferrofed_testkit::perf_token`]) as a process of its own (#1712).
//!
//! Every setting is one `--name value` argument, all of them required:
//! `--listen`, `--persons-issuer`, `--persons-jwks`, `--machines-issuer`,
//! `--machines-jwks`, `--audience`, `--client-id`, `--client-key`,
//! `--caller-issuer`, `--caller-jwks` and `--lifetime-s`. It prints its
//! token URL once it listens and serves until it is stopped.
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a tool binary reports its address and its refusal on stdio"
)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use ferrofed_testkit::perf_token::{PerfTokenEndpoint, Settings};

/// The settings named on the command line, or why they cannot be read.
fn settings() -> Result<Settings, String> {
    let mut named = BTreeMap::new();
    let mut args = std::env::args().skip(1);
    while let Some(name) = args.next() {
        let Some(key) = name.strip_prefix("--") else {
            return Err(format!("{name} is no --name"));
        };
        let Some(value) = args.next() else {
            return Err(format!("{name} has no value"));
        };
        named.insert(key.to_owned(), value);
    }
    let mut take = |key: &str| {
        named
            .remove(key)
            .ok_or_else(|| format!("--{key} is required"))
    };
    let listen = take("listen")?;
    let lifetime = take("lifetime-s")?;
    let settings = Settings {
        listen: listen
            .parse()
            .map_err(|error| format!("--listen {listen}: {error}"))?,
        persons_issuer: take("persons-issuer")?,
        persons_jwks: PathBuf::from(take("persons-jwks")?),
        machines_issuer: take("machines-issuer")?,
        machines_jwks: PathBuf::from(take("machines-jwks")?),
        audience: take("audience")?,
        client_id: take("client-id")?,
        client_key: PathBuf::from(take("client-key")?),
        caller_issuer: take("caller-issuer")?,
        caller_jwks: PathBuf::from(take("caller-jwks")?),
        lifetime_s: lifetime
            .parse()
            .map_err(|error| format!("--lifetime-s {lifetime}: {error}"))?,
    };
    if let Some(key) = named.keys().next() {
        return Err(format!("--{key} is not a setting"));
    }
    Ok(settings)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let settings = match settings() {
        Ok(settings) => settings,
        Err(why) => {
            eprintln!("ferrofed-perf-token: {why}");
            return ExitCode::from(2);
        }
    };
    let endpoint = match PerfTokenEndpoint::start(settings).await {
        Ok(endpoint) => endpoint,
        Err(error) => {
            eprintln!("ferrofed-perf-token: {error}");
            return ExitCode::from(2);
        }
    };
    println!("{}", endpoint.token_url());
    match endpoint.serve().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ferrofed-perf-token: the listener ended: {error}");
            ExitCode::FAILURE
        }
    }
}
