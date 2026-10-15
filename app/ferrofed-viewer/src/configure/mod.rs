// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The configuration page: the gateway's configuration revisions, the
//! members and stored queries with their owners, and the console's changes
//! to the ones it owns.
//!
//! The page is read from the first replica's admin listener with the
//! operator's own access token ([`act`]) and rendered on the server, its
//! forms whole, so the browser receives it as HTML. A member the file's
//! registry document or a care services directory provides is read-only
//! and marked so; a console member is saved as a revision, its changes
//! against the running revision are shown, and it is applied with a
//! confirmation, as a rollback is. A stored-query version is stored once
//! and never edited (§12.7, N44). Every form posts to one server function,
//! which checks the confirmation again; the forms sit in one fieldset that
//! stays disabled until the page has hydrated, and its one listener hands
//! the form submitted to the server function. No specification governs the
//! console: our own design.

// The browser half of each server function, which the server macro writes,
// is a network call that awaits nothing, and the lint names the macro alone.
#[cfg_attr(
    target_arch = "wasm32",
    expect(
        clippy::unused_async_trait_impl,
        reason = "written by the server_fn #[server] macro for the browser half, whose call awaits nothing here (https://docs.rs/server_fn/0.8/server_fn/attr.server.html)"
    )
)]
pub mod act;
pub mod model;
#[cfg(not(target_arch = "wasm32"))]
pub mod render;

use leptos::prelude::*;
use leptos_meta::Title;

use crate::configure::act::Configure;
use crate::configure::model::{ConfigForm, ConfigView};

/// The path of the configuration page.
pub const CONFIGURATION: &str = "/configuration";

/// The configuration page.
#[component]
#[expect(
    clippy::must_use_candidate,
    reason = "the component macro writes the function it returns without the attributes on the one written here (https://docs.rs/leptos/0.8/leptos/attr.component.html)"
)]
pub fn ConfigurationPage() -> impl IntoView {
    let configure = ServerAction::<Configure>::new();
    let version = configure.version();
    let loaded = Resource::new_blocking(move || version.get(), |_| act::configuration());
    // NOTE: the Leptos book, `ssr/24_hydration_bugs`: an effect runs only in the
    // browser, once hydrated, so the server and the hydration render alike.
    let hydrated = RwSignal::new(false);
    Effect::new(move |_| hydrated.set(true));
    let outcome = (configure.value(), configure.pending());
    let pending = outcome.1;
    view! {
        <Title text=crate::views::titled("Configuration") />
        <h1>"Configuration"</h1>
        <Transition fallback=|| {
            view! { <p>"Loading the configuration."</p> }
        }>
            {move || Suspend::new(async move {
                crate::views::shown(
                    loaded.await,
                    move |view| config_section(view, configure, hydrated),
                )
            })}
        </Transition>
        <section aria-labelledby="configure-outcome" aria-busy=move || pending.get().to_string()>
            <h2 id="configure-outcome">"Outcome"</h2>
            {crate::admin::answered(outcome)}
        </section>
    }
}

/// The page the server rendered, its forms in one fieldset that hands the
/// form submitted to `configure` once `hydrated`.
fn config_section(
    view: ConfigView,
    configure: ServerAction<Configure>,
    hydrated: RwSignal<bool>,
) -> AnyView {
    let submit = move |event: leptos::ev::SubmitEvent| {
        let form = event_target::<leptos::web_sys::HtmlFormElement>(&event);
        // NOTE: no specification governs this: our own design; form data the
        // browser cannot give leaves the plain post, which the server refuses.
        if let Ok(data) = leptos::web_sys::FormData::new_with_form(&form) {
            event.prevent_default();
            configure.dispatch(Configure {
                form: submitted(&data),
            });
        }
    };
    view! {
        <div inner_html=view.html></div>
        <fieldset disabled=move || !hydrated.get() on:submit=submit>
            <legend>"Changes"</legend>
            <p role="status">{move || crate::admin::ready(hydrated.get())}</p>
            <div inner_html=view.forms></div>
        </fieldset>
    }
    .into_any()
}

/// The configuration form `data` holds, by the field names the forms post.
fn submitted(data: &leptos::web_sys::FormData) -> ConfigForm {
    // NOTE: XHR standard, `FormData.get`: a field the form does not send, a
    // clear checkbox among them, is null, which is the empty field it means.
    let field = |name: &str| data.get(name).as_string().unwrap_or_default();
    ConfigForm {
        action: field("form[action]"),
        base: field("form[base]"),
        revision: field("form[revision]"),
        current: field("form[current]"),
        reason: field("form[reason]"),
        confirm: field("form[confirm]"),
        organisation: field("form[organisation]"),
        organisation_name: field("form[organisation_name]"),
        node: field("form[node]"),
        system_id: field("form[system_id]"),
        product: field("form[product]"),
        product_version: field("form[product_version]"),
        endpoints: field("form[endpoints]"),
        name: field("form[name]"),
        version: field("form[version]"),
        aql: field("form[aql]"),
    }
}
