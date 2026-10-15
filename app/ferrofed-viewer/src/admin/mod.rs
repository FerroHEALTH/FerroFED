// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operator's admin actions: the clear of a held `ehr_id` beside the
//! integrity incidents, and the stored-query drift repair beside the stored
//! queries.
//!
//! The console's server calls each on a gateway replica's admin listener,
//! at the URL its `[admin]` configuration names, with the operator's own
//! access token. A console configured with no replica shows neither form.
//! Each form asks for a confirmation, which its server function checks
//! again, and posts to the server function of [`act`] in a request body.
//! Its fields are rendered on the server ([`fields`]), and so is its
//! answer ([`answer`]), which names every replica the action went to. No
//! specification governs the admin actions: our own design.

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
#[cfg(not(target_arch = "wasm32"))]
pub mod answer;
#[cfg(not(target_arch = "wasm32"))]
pub mod fields;
pub mod model;

use leptos::html::Form;
use leptos::prelude::*;
use leptos::server_fn::ServerFn as _;

use crate::admin::act::{ClearEhrId, RepairDrift};
use crate::admin::model::{ActionAnswer, AdminForm};
use crate::views::fault;
use crate::views::model::ViewError;

/// What one action's section names: its heading, the server function it
/// posts to, and its submit button.
struct Action {
    /// The id of the section's heading.
    id: &'static str,
    /// The id of the heading of its outcome.
    outcome: &'static str,
    /// The section's heading.
    heading: &'static str,
    /// The path of the server function the form posts to.
    url: &'static str,
    /// The submit button's label.
    label: &'static str,
}

/// The admin action form `data` holds, by the field names the forms post.
fn submitted(data: &leptos::web_sys::FormData) -> AdminForm {
    // NOTE: XHR standard, `FormData.get`: a field the form does not send, a
    // clear checkbox among them, is null, which is the empty field it means.
    let field = |name: &str| data.get(name).as_string().unwrap_or_default();
    AdminForm {
        replica: field("form[replica]"),
        ehr_id: field("form[ehr_id]"),
        name: field("form[name]"),
        version: field("form[version]"),
        endpoints: field("form[endpoints]"),
        confirm: field("form[confirm]"),
    }
}

/// The section of `action`: the server-rendered `fields` in a form that
/// hands what it holds to `dispatch` once the page has hydrated, and the
/// outcome `value` holds while `pending` says whether one is running.
///
/// The form runs only once hydrated: its submit stays disabled until then,
/// and the server function refuses a plain form post before any replica is
/// asked.
fn action_section(
    action: &Action,
    fields: String,
    dispatch: Callback<AdminForm>,
    (value, pending): (
        MappedSignal<Option<Result<ActionAnswer, ViewError>>>,
        Memo<bool>,
    ),
) -> AnyView {
    let form = NodeRef::<Form>::new();
    // NOTE: the Leptos book, `ssr/24_hydration_bugs`: an effect runs only in the
    // browser, once hydrated, so the server and the hydration render alike.
    let hydrated = RwSignal::new(false);
    Effect::new(move |_| hydrated.set(true));
    let submit = move |event: leptos::ev::SubmitEvent| {
        let data = form
            .get_untracked()
            .and_then(|form| leptos::web_sys::FormData::new_with_form(&form).ok());
        // NOTE: no specification governs this: our own design; form data the
        // browser cannot give leaves the plain post, which the server refuses.
        if let Some(data) = data {
            event.prevent_default();
            dispatch.run(submitted(&data));
        }
    };
    view! {
        <section aria-labelledby=action.id>
            <h2 id=action.id>{action.heading}</h2>
            <form method="post" action=action.url node_ref=form on:submit=submit>
                <div inner_html=fields></div>
                <button type="submit" disabled=move || !hydrated.get()>
                    {action.label}
                </button>
                <p role="status">
                    {move || {
                        if hydrated.get() {
                            ""
                        } else {
                            "The form is ready once the page has loaded."
                        }
                    }}
                </p>
            </form>
            <section aria-labelledby=action.outcome aria-busy=move || pending.get().to_string()>
                <h3 id=action.outcome>"Outcome"</h3>
                <p role="status">
                    {move || if pending.get() { "Running the action." } else { "" }}
                </p>
                {move || {
                    value
                        .with(|answer| match answer {
                            None => view! { <p>"No action has run yet."</p> }.into_any(),
                            Some(Ok(rendered)) => {
                                let html = rendered.html.clone();
                                view! { <div inner_html=html></div> }.into_any()
                            }
                            Some(Err(error)) => fault(error),
                        })
                }}
            </section>
        </section>
    }
    .into_any()
}

/// The collision clear beside the integrity incidents, its fields the
/// server rendered, or nothing when the console offers no admin action.
pub(crate) fn clear_section(action: ServerAction<ClearEhrId>, fields: Option<String>) -> AnyView {
    let Some(fields) = fields else {
        return ().into_any();
    };
    let named = Action {
        id: "clear",
        outcome: "clear-outcome",
        heading: "Clear a held ehr_id",
        url: ClearEhrId::url(),
        label: "Clear the ehr_id",
    };
    let dispatch = Callback::new(move |form| {
        action.dispatch(ClearEhrId { form });
    });
    action_section(&named, fields, dispatch, (action.value(), action.pending()))
}

/// The drift repair beside the stored queries, its fields the server
/// rendered, or nothing when the console offers no admin action.
pub(crate) fn repair_section(action: ServerAction<RepairDrift>, fields: Option<String>) -> AnyView {
    let Some(fields) = fields else {
        return ().into_any();
    };
    let named = Action {
        id: "repair",
        outcome: "repair-outcome",
        heading: "Repair stored-query drift",
        url: RepairDrift::url(),
        label: "Send the version again",
    };
    let dispatch = Callback::new(move |form| {
        action.dispatch(RepairDrift { form });
    });
    action_section(&named, fields, dispatch, (action.value(), action.pending()))
}
