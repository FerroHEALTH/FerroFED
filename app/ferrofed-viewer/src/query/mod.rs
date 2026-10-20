// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The query console.
//!
//! An AQL query, or a stored query by name, runs through the gateway as the
//! signed-in operator, and every endpoint's status and whether the answer is
//! complete show beside the rows (§9, §11.1, §11.4, N37).
//!
//! The form posts to the server function of [`run`] in a request body, so
//! nothing the operator enters, a patient named in a parameter among it,
//! reaches a URL, the browser history or a log (N33). The answer renders in
//! the page that sent it, and an incomplete one says so in words before its
//! rows.

#[cfg(not(target_arch = "wasm32"))]
pub mod answer;
mod editor;
pub mod model;
mod results;
// The browser half of each server function, which the server macro writes,
// is a network call that awaits nothing, and the lint names the macro alone.
#[cfg_attr(
    target_arch = "wasm32",
    expect(
        clippy::unused_async_trait_impl,
        reason = "written by the server_fn #[server] macro for the browser half, whose call awaits nothing here (https://docs.rs/server_fn/0.8/server_fn/attr.server.html)"
    )
)]
pub mod run;
mod tabs;

use leptos::prelude::*;
use leptos_meta::Title;

use crate::locale::{Msg, tr};
use crate::query::run::RunQuery;
use crate::views::model::Outcome;
use crate::views::{fault, refusal, titled};
use ferrohealth_ui::view::empty::{Busy, busy};
use ferrohealth_ui::view::surface::page_header;

/// The path of the query console.
pub const QUERY: &str = "/query";

/// The query console: the editor card, and the results card under it.
#[component]
#[expect(
    clippy::must_use_candidate,
    reason = "the component macro writes the function it returns without the attributes on the one written here (https://docs.rs/leptos/0.8/leptos/attr.component.html)"
)]
pub fn QueryPage() -> impl IntoView {
    let options = Resource::new_blocking(|| (), |()| run::query_options());
    let action = ServerAction::<RunQuery>::new();
    let header = page_header(
        tr(Msg::QueryConsole),
        Some(tr(Msg::RunAqlStoredQueryAcrossFederation)),
        (),
    )
    .into_any();
    let results = results::results_section(action);
    view! {
        <Title text=titled(&tr(Msg::Query)) />
        {header}
        <div class="fui-stack">
            <Suspense fallback=|| {
                busy(tr(Msg::LoadingWhatGatewayOffersQuery), Busy::Section)
            }>
                {move || Suspend::new(async move {
                    match options.await {
                        Ok(Outcome::Shown(offered)) => editor::editor_section(action, &offered),
                        Ok(Outcome::Refused(refused)) => refusal(&refused),
                        Err(error) => fault(&error),
                    }
                })}
            </Suspense>
            {results}
        </div>
    }
}
