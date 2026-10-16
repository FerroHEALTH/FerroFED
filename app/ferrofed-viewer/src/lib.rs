// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FerroFED Console, the operator console: a Leptos app rendered on the server and
//! hydrated in the browser, with a backend-for-frontend in front of the
//! gateway.
//!
//! The viewer is one more client of the gateway's public surface: it reaches
//! the gateway over HTTP alone, through [`gateway`], and never links the
//! engine. It holds the operator's sign-in session on the server
//! ([`session`], [`oidc`]), so the browser carries an opaque cookie and never
//! a token, and it holds no clinical data. [`admin`] runs the operator's
//! admin actions on the replicas' admin listeners its configuration names.
//! [`app`] is the page tree both halves share, drawn in the [`shell`] from
//! the [`components`] and [`styles`] in the operator's [`theme`], with
//! [`manufacturer`] named in full on the [`about`] page every footer links
//! to; [`server`] serves it with the health route and the sign-in
//! routes, [`config`] reads the TOML configuration, [`cli`] and [`command`]
//! run the binary, and [`healthcheck`] is the job a container runtime runs
//! beside it. No specification governs the viewer: our own design.
//!
//! The server half compiles for the host and the browser half for
//! `wasm32-unknown-unknown`; the target, never a Cargo feature, chooses
//! between them.
#![doc(test(attr(deny(warnings))))]

pub mod about;
pub mod admin;
pub mod app;
pub mod components;
pub mod configure;
pub mod home;
// NOTE: Regulation (EU) 2025/327 Art 30(1)(g): the browser half links no FerroFED crate,
// so the console compiles the registry's own file (the Rust reference, the `path` attribute).
#[path = "../../ferrofed-registry/src/manufacturer.rs"]
pub mod manufacturer;
pub mod query;
pub mod shell;
pub mod styles;
// The browser half of the server function, which the server macro writes, is
// a network call that awaits nothing, and the lint names the macro alone.
#[cfg_attr(
    target_arch = "wasm32",
    expect(
        clippy::unused_async_trait_impl,
        reason = "written by the server_fn #[server] macro for the browser half, whose call awaits nothing here (https://docs.rs/server_fn/0.8/server_fn/attr.server.html)"
    )
)]
pub mod theme;
pub mod views;

#[cfg(not(target_arch = "wasm32"))]
pub mod cli;
#[cfg(not(target_arch = "wasm32"))]
pub mod command;
#[cfg(not(target_arch = "wasm32"))]
pub mod config;
#[cfg(not(target_arch = "wasm32"))]
pub mod dashboard;
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
