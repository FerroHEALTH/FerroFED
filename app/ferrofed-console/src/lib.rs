// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FerroFED Console, the operator console: a Leptos app the server
//! draws and the browser hydrates whole, drawing every page after the first
//! itself, with a backend-for-frontend in front of the gateway.
//!
//! The console is one more client of the gateway's public surface: its
//! server reaches the gateway over HTTP alone, through [`gateway`], and
//! never links the engine. It holds the operator's sign-in session on the
//! server ([`session`], [`oidc`]), so the browser carries an opaque cookie
//! and never a token, and it holds no clinical data. [`app`] is the page
//! tree, drawn in the [`shell`] with the FerroUI components
//! (`ferrohealth_ui`), [`pages`] the pages, grouped into lazy routes by
//! [`routes`], [`read`] what each page reads of the gateway behind its
//! server function, [`visit`] what the server read of the request,
//! [`act`] the actions the operator posts, [`configure`] the configuration
//! revisions, [`prefs`] the operator's preferences kept in cookies, and
//! [`locale`] the console's words. [`server`] serves it with the health and
//! sign-in routes, [`config`] reads the TOML configuration, and [`cli`] and
//! [`command`] run the binary, its `healthcheck` the family's from
//! `ferrohealth-server`.
//!
//! Every page works with no WebAssembly: a link is a plain `<a>`, a
//! preference a plain form the server answers, and every action a server
//! function behind an `<ActionForm>` whose plain post is drawn on its page;
//! once the page has hydrated, the form calls the function and a link is
//! followed in the page. The server half compiles for the host and the
//! browser half for `wasm32-unknown-unknown`; the target, never a Cargo
//! feature, chooses between them. No specification governs the console:
//! our own design.
#![doc(test(attr(deny(warnings))))]

pub mod act;
pub mod app;
pub mod configure;
pub mod editors;
pub mod locale;
pub mod pages;
pub mod prefs;
pub mod read;
pub mod routes;
pub mod shell;
pub mod visit;

#[cfg(not(target_arch = "wasm32"))]
pub mod cli;
#[cfg(not(target_arch = "wasm32"))]
pub mod command;
#[cfg(not(target_arch = "wasm32"))]
pub mod config;
#[cfg(not(target_arch = "wasm32"))]
pub mod gateway;
#[cfg(not(target_arch = "wasm32"))]
pub mod oidc;
#[cfg(not(target_arch = "wasm32"))]
pub mod server;
#[cfg(not(target_arch = "wasm32"))]
pub mod session;

/// Hydrates the page the server drew, whole, through FerroUI's entry.
///
/// Its lazy routes load their code from their own chunks, and the root
/// element takes `data-ferroui-hydrated` once the page and its route have
/// hydrated.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    ferrohealth_ui::widget::hydration::explain();
    ferrohealth_ui::widget::hydration::hydrate(app::App);
}
