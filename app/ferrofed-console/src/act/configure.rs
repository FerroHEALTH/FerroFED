// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The post of a configuration form.
//!
//! The fields are read into a [`crate::configure::model::ConfigForm`] and
//! handed to the page, which runs the form as it is drawn again and shows
//! its answer at the top (`crate::configure::act::run`).

use axum::Extension;
use axum::extract::Form;
use axum::response::Response;
use http::{HeaderMap, Uri};
use leptos::context::provide_context;

use crate::act::render;
use crate::configure::model::ConfigForm;
use crate::server::ConsoleState;

/// The route an import of TOML posts to.
pub const IMPORT: &str = "/configuration/import-export";

/// A configuration form as it was posted, for the page that runs it.
#[derive(Debug, Clone)]
pub struct ConfigPost(pub ConfigForm);

/// Serves the post of a configuration form.
pub async fn post(
    Extension(state): Extension<ConsoleState>,
    headers: HeaderMap,
    extensions: http::Extensions,
    uri: Uri,
    Form(fields): Form<Vec<(String, String)>>,
) -> Response {
    let posted = ConfigPost(ConfigForm::of(&fields));
    render(&state, (headers, extensions), uri, move || {
        provide_context(posted.clone());
    })
    .await
}
