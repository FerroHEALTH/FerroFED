// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operator's admin actions: the clear of a held `ehr_id` beside the
//! integrity incidents, and the stored-query drift repair beside the stored
//! queries.
//!
//! The console's server calls each on a gateway replica's admin listener,
//! at the URL its `[admin]` configuration names, with the operator's own
//! access token. A console configured with no replica shows neither form.
//! Each action is a card whose button opens a confirmation step, whose
//! server function checks the confirmation again, and which posts to the
//! server function of [`act`] in a request body. Its fields are rendered on
//! the server ([`fields`]), and so is its answer ([`answer`]), which names
//! every replica the action went to and shows at the top of the card, the
//! focus moved there. No specification governs the admin actions: our own
//! design.

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

use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::empty::{Busy, busy};
use ferrohealth_ui::view::surface::well;
use leptos::html::{Div, Form};
use leptos::prelude::*;
use leptos::server_fn::ServerFn as _;

use crate::admin::act::{ClearEhrId, RepairDrift};
use crate::admin::model::{ActionAnswer, AdminForm};
use crate::views::fault;
use crate::views::model::ViewError;

/// The book page that explains the operator console's admin actions.
const LEARN: &str = "https://ferrofed.eu/docs/operate/operator-console.html";

/// What one action's card names: its heading, its one line, the server
/// function it posts to, its button and its confirmation.
struct Action {
    /// The id of the card's heading; the confirmation is `{id}-confirm`.
    id: &'static str,
    /// The id of its outcome, which the focus moves to.
    outcome: &'static str,
    /// The card's heading.
    heading: &'static str,
    /// The one line under it.
    line: &'static str,
    /// The path of the server function the form posts to.
    url: &'static str,
    /// The label of the button that opens the confirmation and of the one
    /// that runs the action.
    label: &'static str,
    /// What the confirmation asks.
    question: &'static str,
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

/// Closes the confirmation step `popover` once its form is sent.
fn closed(popover: NodeRef<Div>) {
    #[cfg(target_arch = "wasm32")]
    if let Some(popover) = popover.get_untracked()
        && popover.matches(":popover-open").unwrap_or(false)
        && popover.hide_popover().is_err()
    {
        leptos::logging::warn!("the browser refused to close the confirmation");
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _: NodeRef<Div> = popover;
}

/// The card of `action`: the server-rendered `fields` in a form that hands
/// what it holds to `dispatch` once the page has hydrated and its
/// confirmation step is taken, and the outcome `value` holds at the top of
/// the card while `pending` says whether one is running.
///
/// The form runs only once hydrated: its buttons stay disabled until then,
/// and the server function refuses a plain form post before any replica is
/// asked.
fn action_section(
    action: &Action,
    fields: String,
    dispatch: Callback<AdminForm>,
    outcome: Outcome,
) -> AnyView {
    let form = NodeRef::<Form>::new();
    let popover = NodeRef::<Div>::new();
    let shown = NodeRef::<Div>::new();
    // NOTE: the Leptos book, `ssr/24_hydration_bugs`: an effect runs only in the
    // browser, once hydrated, so the server and the hydration render alike.
    let hydrated = RwSignal::new(false);
    Effect::new(move |_| hydrated.set(true));
    let (value, pending) = outcome;
    // NOTE: WCAG 2.2 SC 2.4.3: focus follows an answer to the top of its card.
    Effect::new(move |_| {
        let answered = value.with(Option::is_some);
        if let Some(shown) = shown.get()
            && answered
        {
            match shown.focus() {
                Ok(()) | Err(_) => {}
            }
        }
    });
    let submit = move |event: leptos::ev::SubmitEvent| {
        let data = form
            .get_untracked()
            .and_then(|form| leptos::web_sys::FormData::new_with_form(&form).ok());
        // NOTE: no specification governs this: our own design; form data the
        // browser cannot give leaves the plain post, which the server refuses.
        if let Some(data) = data {
            event.prevent_default();
            closed(popover);
            dispatch.run(submitted(&data));
        }
    };
    let confirm = format!("{}-confirm", action.id);
    let question = format!("{}-question", action.id);
    let (opens, closes, labelled) = (confirm.clone(), confirm.clone(), question.clone());
    view! {
        <section class="fui-card" aria-labelledby=action.id>
            <h2 id=action.id class="fui-card-title">
                {action.heading}
            </h2>
            <p class="fui-muted">
                {action.line} " " <a href=LEARN rel="external">
                    "Learn more"
                </a>
            </p>
            <div
                id=action.outcome
                node_ref=shown
                tabindex="-1"
                aria-busy=move || pending.get().to_string()
            >
                {answered((value, pending))}
            </div>
            <form method="post" action=action.url node_ref=form on:submit=submit class="fui-form">
                <div inner_html=fields></div>
                <div class="fui-cluster">
                    <button
                        type="button"
                        class=Variant::Primary.class()
                        popovertarget=opens
                        aria-haspopup="dialog"
                        disabled=move || !hydrated.get()
                    >
                        {action.label}
                    </button>
                </div>
                <div
                    id=confirm
                    popover="auto"
                    role="dialog"
                    aria-labelledby=labelled
                    node_ref=popover
                    class="fui-dialog"
                >
                    <h2 id=question class="fui-dialog-title">
                        {action.question}
                    </h2>
                    <input type="hidden" name="form[confirm]" value="yes" />
                    <div class="fui-dialog-actions">
                        <button
                            type="button"
                            class=Variant::Secondary.class()
                            popovertarget=closes
                            popovertargetaction="hide"
                        >
                            "Cancel"
                        </button>
                        <button
                            type="submit"
                            class=Variant::Primary.class()
                            autofocus
                            disabled=move || !hydrated.get()
                        >
                            {action.label}
                        </button>
                    </div>
                </div>
                <p role="status" class="fui-help">
                    {move || ready(hydrated.get())}
                </p>
            </form>
        </section>
    }
    .into_any()
}

/// What a form says while the page has not hydrated, and nothing once it
/// has.
pub(crate) fn ready(hydrated: bool) -> &'static str {
    if hydrated {
        ""
    } else {
        "The form is ready once the page has loaded."
    }
}

/// The answer an action holds and whether one is running.
type Outcome = (
    MappedSignal<Option<Result<ActionAnswer, ViewError>>>,
    Memo<bool>,
);

/// Whether an action runs, and the answer the server rendered for the last
/// one, or the console's fault; nothing before the first.
fn answered((value, pending): Outcome) -> AnyView {
    view! {
        {move || {
            pending.get().then(|| busy("Running the action.", Busy::Inline))
        }}
        {move || {
            value
                .with(|answer| match answer {
                    None => ().into_any(),
                    Some(Ok(rendered)) => {
                        let html = rendered.html.clone();
                        well(view! { <div inner_html=html></div> }).into_any()
                    }
                    Some(Err(error)) => fault(error),
                })
        }}
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
        line: "Forgets a remedied collision at a replica, so the next request finds the owner again.",
        url: ClearEhrId::url(),
        label: "Clear the ehr_id",
        question: "Clear this ehr_id at the replica chosen? Each replica holds its own routing state.",
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
        line: "Sends the registry's held copy of a version again to the members named.",
        url: RepairDrift::url(),
        label: "Send the version again",
        question: "Send this version again to the members named? The registry's copy stays as it is.",
    };
    let dispatch = Callback::new(move |form| {
        action.dispatch(RepairDrift { form });
    });
    action_section(&named, fields, dispatch, (action.value(), action.pending()))
}
