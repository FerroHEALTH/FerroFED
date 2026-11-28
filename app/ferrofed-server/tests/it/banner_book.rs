// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The startup banner sample of the configuration page, rendered from the
//! quickstart's development configuration for the version being built.
//!
//! `the_book_holds_the_banner_the_quickstart_prints` fails when the sample
//! differs from what [`render`] prints, so a release that moves the version
//! or a pin moves the page with it; `write_the_banner_sample` (ignored, run
//! with `cargo nextest run -p ferrofed-server --test it --run-ignored only
//! banner_book`) writes the sample again. The quickstart resolves patients
//! through a PIX Manager, so the module builds with the IHE binding alone.
//! No specification governs the page: our own design.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};

use ferrofed_server::banner::{Deployment, render};
use ferrofed_server::config::Config;
use ferrofed_server::federation::registry::read_registry;
use ferrofed_server::support::Support;
use ferrohealth_telemetry::section::Rendering;

use crate::support::signing_key_file;

type TestResult = Result<(), Box<dyn Error>>;

/// The page, relative to the workspace root.
const PAGE: &str = "website/book/src/operate/configuration.md";

/// The heading the sample sits under.
const HEADING: &str = "### The startup banner";

/// The variable `compose.yaml` sets the gateway's listen address with.
const LISTEN: &str = "FERROFED__SERVER__LISTEN";

/// Returns `path`, relative to the workspace root, as a full path.
fn workspace(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

/// Returns the listen address `compose.yaml` gives the gateway.
fn compose_listen() -> Result<String, Box<dyn Error>> {
    let compose = std::fs::read_to_string(workspace("compose.yaml"))?;
    let value = compose
        .lines()
        .find_map(|line| line.trim().strip_prefix(LISTEN)?.strip_prefix(':'))
        .ok_or("compose.yaml sets the gateway's listen address")?;
    Ok(value.trim().to_owned())
}

/// Returns the banner the quickstart's gateway prints, built from source.
///
/// The configuration's container paths name the quickstart's registry
/// document and the suite's signing key, which stands in for the key a
/// reader writes before the first `docker compose up`.
fn quickstart_banner() -> Result<String, Box<dyn Error>> {
    let quickstart = workspace("docker/quickstart");
    let registry = quickstart.join("registry.toml");
    let text = std::fs::read_to_string(quickstart.join("ferrofed.toml"))?
        .replace("/etc/ferrofed/registry.toml", &path_text(&registry)?)
        .replace("/run/secrets/signing-key", signing_key_file());
    let environment = BTreeMap::from([(LISTEN.to_owned(), compose_listen()?)]);
    let settings = Config::from_sources(Some(&text), &environment)?.resolve()?;
    let read = read_registry(&settings);
    let deployment = Deployment::serving(&settings, read.as_ref().map(Result::as_ref))
        .with_support(Support::NoPeriod);
    let banner = render(
        env!("CARGO_PKG_VERSION"),
        &deployment,
        false,
        Rendering::Pretty,
    )
    .ok_or("pretty renders the banner")?;
    Ok(banner.trim_start_matches('\n').trim_end().to_owned())
}

/// Returns `path` as text, refusing one that is not UTF-8.
fn path_text(path: &Path) -> Result<String, Box<dyn Error>> {
    Ok(path.to_str().ok_or("the path is UTF-8")?.to_owned())
}

/// Splits `page` around the sample's text: what precedes it, the sample,
/// and what follows it.
fn split(page: &str) -> Result<(&str, &str, &str), Box<dyn Error>> {
    let section = page
        .find(HEADING)
        .ok_or("the page has the banner section")?;
    let open = page
        .get(section..)
        .and_then(|rest| rest.find("```text\n"))
        .map(|at| section + at + "```text\n".len())
        .ok_or("the banner section opens a text block")?;
    let close = page
        .get(open..)
        .and_then(|rest| rest.find("\n```\n"))
        .map(|at| open + at)
        .ok_or("the banner's text block closes")?;
    let before = page.get(..open).ok_or("the block opens on a boundary")?;
    let sample = page.get(open..close).ok_or("the block is text")?;
    let after = page.get(close..).ok_or("the block closes on a boundary")?;
    Ok((before, sample, after))
}

#[test]
fn the_book_holds_the_banner_the_quickstart_prints() -> TestResult {
    let page = std::fs::read_to_string(workspace(PAGE))?;
    let (_, sample, _) = split(&page)?;
    let banner = quickstart_banner()?;
    if sample != banner {
        return Err(format!(
            "{PAGE} shows another banner than the quickstart prints; run `cargo nextest run \
             -p ferrofed-server --test it --run-ignored only banner_book` to write it:\n{banner}"
        )
        .into());
    }
    Ok(())
}

#[test]
#[ignore = "writes the banner sample of the configuration page"]
fn write_the_banner_sample() -> TestResult {
    let path = workspace(PAGE);
    let page = std::fs::read_to_string(&path)?;
    let (before, _, after) = split(&page)?;
    let written = [before, &quickstart_banner()?, after].concat();
    std::fs::write(path, written)?;
    Ok(())
}
