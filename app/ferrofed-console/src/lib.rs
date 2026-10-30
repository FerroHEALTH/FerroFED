// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FerroFED Console, the operator console: pages the server draws
//! whole with FerroUI's components, with a backend-for-frontend in front of
//! the gateway.
//!
//! The console is one more client of the gateway's public surface: its
//! server reaches the gateway over HTTP alone, through [`gateway`], and
//! never links the engine. It holds the operator's sign-in session on the
//! server ([`session`], [`oidc`]), so the browser carries an opaque cookie
//! and never a token, and it holds no clinical data. [`routes`] draws the
//! page an address names, in the [`shell`] with the FerroUI components
//! (`ferrohealth_ui`) and the document of [`app`]; [`pages`] are the pages,
//! [`read`] what each page reads of the gateway, [`visit`] what the server
//! read of the request, [`act`] the actions the operator posts,
//! [`configure`] the configuration revisions, [`prefs`] the operator's
//! preferences kept in cookies, and [`locale`] the console's words.
//! [`server`] serves it with the health and sign-in routes and FerroUI's
//! assets, [`config`] reads the TOML configuration, and [`cli`] and
//! [`command`] run the binary, its `healthcheck` the family's from
//! `ferrohealth-server`.
//!
//! Every page works with the scripts refused: a link is a plain `<a>`, a
//! preference a plain form the server answers, and every action a form
//! that posts to its page, which the server draws again with the answer.
//! FerroUI's script modules add the in-page behaviour on top. No
//! specification governs the console: our own design.
#![doc(test(attr(deny(warnings))))]

pub mod act;
pub mod app;
pub mod cli;
pub mod command;
pub mod config;
pub mod configure;
pub mod editors;
pub mod gateway;
pub mod locale;
pub mod oidc;
pub mod pages;
pub mod prefs;
pub mod read;
pub mod routes;
pub mod server;
pub mod session;
pub mod shell;
pub mod visit;
