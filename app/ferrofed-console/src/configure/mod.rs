// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The configuration: the gateway's configuration revisions, the members,
//! credentials, binding sections and stored queries with their owners, and
//! the console's changes to the ones it owns.
//!
//! The page (T3) has five binder tabs, each at its own path under
//! `/configuration`: Members, Credentials and bindings, Stored queries,
//! History, and Import and export; a member's, a credential's, a binding
//! section's and a stored query's form is a page of its own under its tab.
//! Above the tabs sits the card of the revision saved and not applied,
//! with what it changes and its apply, and of the confirmation the running
//! revision awaits. A change is saved as a revision and applied
//! separately; an apply, a roll back, a removal and a stored version are
//! each confirmed in a step of their own.
//!
//! The page is read from the first replica's admin listener with the
//! operator's own access token (`act`, behind `crate::act::configure`) and
//! drawn whole, its forms whole. Every form posts to the page it sits on,
//! read on the server into a [`model::ConfigForm`] and run, so its answer
//! shows at the top of the page; a refusal of one field names that field. No specification governs the console: our own design.
pub mod act;
pub mod draft;
pub mod export;
pub mod import;
pub mod member;
pub mod model;
pub mod plan;
pub mod render;
pub mod said;
pub mod sections;
pub mod settings;

/// The path of the configuration page.
pub const CONFIGURATION: &str = "/configuration";
