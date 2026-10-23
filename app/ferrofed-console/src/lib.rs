// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FerroFED Console, the operator console: a Leptos app rendered on the
//! server in islands mode, with a backend-for-frontend in front of the
//! gateway.
//!
//! The console is one more client of the gateway's public surface: it
//! reaches the gateway over HTTP alone, through [`gateway`], and never links
//! the engine. It holds the operator's sign-in session on the server
//! ([`session`], [`oidc`]), so the browser carries an opaque cookie and
//! never a token, and it holds no clinical data. [`app`] is the page tree,
//! drawn in the [`shell`] with the FerroUI components (`ferrohealth_ui`),
//! [`pages`] the pages, [`read`] what each page reads of the gateway,
//! [`act`] the forms the operator posts, [`prefs`] the operator's
//! preferences kept in cookies, and [`locale`] the console's words.
//! [`server`] serves it with the health and sign-in routes, [`config`]
//! reads the TOML configuration, [`cli`] and [`command`] run the binary,
//! and [`healthcheck`] is the job a container runtime runs beside it.
//!
//! The server draws every page whole and the browser hydrates only the
//! islands FerroUI's components and [`islands`] carry (the Leptos book,
//! "Guide: Islands"), so every page reads and every action posts with no
//! WebAssembly. The server half compiles for the host and the browser half
//! for `wasm32-unknown-unknown`; the target, never a Cargo feature, chooses
//! between them. No specification governs the console: our own design.
#![doc(test(attr(deny(warnings))))]

pub mod islands;
// NOTE: Regulation (EU) 2025/327 Art 30(1)(g): the console links no FerroFED crate in
// its browser half, so it compiles the registry's own file (the Rust reference, `path`).
#[path = "../../ferrofed-registry/src/manufacturer.rs"]
pub mod manufacturer;

#[cfg(not(target_arch = "wasm32"))]
pub mod act;
#[cfg(not(target_arch = "wasm32"))]
pub mod app;
#[cfg(not(target_arch = "wasm32"))]
pub mod cli;
#[cfg(not(target_arch = "wasm32"))]
pub mod command;
#[cfg(not(target_arch = "wasm32"))]
pub mod config;
#[cfg(not(target_arch = "wasm32"))]
pub mod configure;
#[cfg(not(target_arch = "wasm32"))]
pub mod gateway;
#[cfg(not(target_arch = "wasm32"))]
pub mod healthcheck;
#[cfg(not(target_arch = "wasm32"))]
pub mod locale;
#[cfg(not(target_arch = "wasm32"))]
pub mod oidc;
#[cfg(not(target_arch = "wasm32"))]
pub mod pages;
#[cfg(not(target_arch = "wasm32"))]
pub mod prefs;
#[cfg(not(target_arch = "wasm32"))]
pub mod read;
#[cfg(not(target_arch = "wasm32"))]
pub mod server;
#[cfg(not(target_arch = "wasm32"))]
pub mod session;
#[cfg(not(target_arch = "wasm32"))]
pub mod shell;

/// Hydrates the islands of the page the server rendered. Each island takes
/// the words it draws as props, so the browser builds no locale.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_islands();
}
