// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operator console's browser journeys, behind the `FERROFED_JOURNEYS`
//! gate (#608).
//!
//! Each journey drives headless Chrome over `WebDriver`, through a chromedriver
//! already running, against a running gateway over two stub nodes and a
//! running console that serves its release site bundle, so the browser
//! hydrates the pages the server rendered. The operator signs in at a test
//! OpenID Provider. Each journey fails on any error the browser logs to its
//! console, and waits on elements explicitly, never on a timer.
//!
//! The gate is `FERROFED_JOURNEYS=1`; without it each journey returns at
//! once, so the ordinary suite needs no browser. The `journeys (browser)` job
//! of `.github/workflows/ci.yml` sets it, starts the pinned chromedriver and
//! builds the bundle, and `scripts/checks/e2e-placement.sh` keeps every gated
//! journey in this module. No specification governs the journeys: our own
//! design.

mod browser;
mod provider;
mod query;
mod sign_in;
mod sign_out;
mod stack;
mod views;

/// The environment variable that opts in to the browser journeys.
const JOURNEYS_GATE: &str = "FERROFED_JOURNEYS";

/// Whether the browser journeys run: only when [`JOURNEYS_GATE`] is `1`.
fn journeys_enabled() -> bool {
    std::env::var(JOURNEYS_GATE).is_ok_and(|value| value == "1")
}
