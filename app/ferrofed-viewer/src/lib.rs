// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FerroFED operator console: a Leptos app rendered on the server and
//! hydrated in the browser, with a backend-for-frontend in front of the
//! gateway.
//!
//! The viewer is one more client of the gateway's public surface: it reaches
//! the gateway over HTTP alone, through [`gateway`], and never links the
//! engine. It holds the operator's sign-in session on the server
//! ([`session`], [`oidc`]), so the browser carries an opaque cookie and never
//! a token, and it holds no clinical data. [`app`] is the page tree both
//! halves share, with [`manufacturer`] named in its footer; [`server`] serves it with the health route and the sign-in
//! routes, [`config`] reads the TOML configuration, [`cli`] and [`command`]
//! run the binary, and [`healthcheck`] is the job a container runtime runs
//! beside it. No specification governs the viewer: our own design.
//!
//! The server half compiles for the host and the browser half for
//! `wasm32-unknown-unknown`; the target, never a Cargo feature, chooses
//! between them.
#![doc(test(attr(deny(warnings))))]

pub mod app;
// NOTE: Regulation (EU) 2025/327 Art 30(1)(g): the browser half links no FerroFED crate,
// so the console compiles the registry's own file (the Rust reference, the `path` attribute).
#[path = "../../ferrofed-registry/src/manufacturer.rs"]
pub mod manufacturer;
pub mod query;
pub mod views;

#[cfg(not(target_arch = "wasm32"))]
pub mod cli;
#[cfg(not(target_arch = "wasm32"))]
pub mod command;
#[cfg(not(target_arch = "wasm32"))]
pub mod config;
#[cfg(not(target_arch = "wasm32"))]
pub mod gateway;
#[cfg(not(target_arch = "wasm32"))]
pub mod healthcheck;
#[cfg(not(target_arch = "wasm32"))]
pub mod oidc;
#[cfg(not(target_arch = "wasm32"))]
pub mod server;
#[cfg(not(target_arch = "wasm32"))]
pub mod session;

/// Attaches the browser half to the page the server rendered.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(app::App);
}
