// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The configuration page: the gateway's configuration revisions, the
//! members, credentials, binding sections and stored queries with their
//! owners, and the console's changes to the ones it owns.
//!
//! The page has five tabs, each at its own path under [`CONFIGURATION`]:
//! Members, Credentials and bindings, Stored queries, History, and Import
//! and export; a member's, a credential's, a binding section's and a
//! stored query's form is a page of its own under its tab. Above every tab
//! sits the card of the revision saved and not applied, with what it
//! changes and its apply, and of the confirmation the running revision
//! awaits. A change is saved as a revision and applied separately; an
//! apply, a roll back, a removal and a stored version are each confirmed in
//! a step of their own.
//!
//! The page is read from the first replica's admin listener with the
//! operator's own access token ([`act`]) and rendered on the server, its
//! forms whole, so the browser receives it as HTML. Every form posts to one
//! server function; the forms sit in one fieldset that stays disabled until
//! the page has hydrated, and its listeners hand the form submitted to the
//! server function, add and remove a form's repeatable rows, move between
//! the tabs from the keyboard, and count an import's lines (`browser`).
//! A form's answer shows at the top of its own card, and the focus moves
//! there; a refusal of one field shows under that field. No specification
//! governs the console: our own design.

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
mod browser;
#[cfg(not(target_arch = "wasm32"))]
pub mod export;
#[cfg(not(target_arch = "wasm32"))]
pub mod import;
#[cfg(not(target_arch = "wasm32"))]
pub mod member;
pub mod model;
#[cfg(not(target_arch = "wasm32"))]
pub mod plan;
#[cfg(not(target_arch = "wasm32"))]
pub mod render;
#[cfg(not(target_arch = "wasm32"))]
pub mod sections;
#[cfg(not(target_arch = "wasm32"))]
pub mod settings;

use ferrohealth_ui::view::empty::{Busy, busy};
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::configure::act::Configure;
use crate::configure::model::{ConfigView, Screen};
use crate::locale::{Msg, tr};

/// The path of the configuration page.
pub const CONFIGURATION: &str = "/configuration";

/// The start of every path under the configuration page.
pub const UNDER: &str = "/configuration/";

/// `remaining` milliseconds in words, as the countdown of a revision that
/// awaits its confirmation shows them.
#[must_use]
pub fn countdown(remaining: i64) -> String {
    if remaining <= 0 {
        return tr(Msg::TheDeadlineHasPassed);
    }
    let seconds = remaining.div_euclid(1000);
    format!(
        "{}:{:02} left",
        seconds.div_euclid(60),
        seconds.rem_euclid(60)
    )
}

/// Which card of the page a form's answer shows in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    /// The card of the saved revision and the confirmation awaited.
    Pending,
    /// The tab's main card.
    Main,
}

/// The configuration page.
#[component]
#[expect(
    clippy::must_use_candidate,
    reason = "the component macro writes the function it returns without the attributes on the one written here (https://docs.rs/leptos/0.8/leptos/attr.component.html)"
)]
pub fn ConfigurationPage() -> impl IntoView {
    let configure = ServerAction::<Configure>::new();
    let (version, value) = (configure.version(), configure.value());
    // NOTE: WCAG 2.2 SC 3.3.7 and 2.4.3: only a change the replica made reads the
    // page again, so a refused form keeps what the operator entered.
    let changes = Memo::new(move |previous: Option<&usize>| {
        let at = version.get();
        let changed = value.with(|answer| matches!(answer, Some(Ok(answer)) if answer.changed));
        if changed {
            at
        } else {
            previous.copied().unwrap_or_default()
        }
    });
    let params = use_params_map();
    let query = use_query_map();
    let screen = Memo::new(move |_| Screen {
        tab: params.with(|params| params.get("tab").unwrap_or_default()),
        item: params.with(|params| params.get("item").unwrap_or_default()),
        compare: query.with(|query| query.get("compare").unwrap_or_default()),
    });
    let loaded = Resource::new_blocking(
        move || (screen.get(), changes.get()),
        |(screen, _)| act::configuration(screen),
    );
    // NOTE: the Leptos book, `ssr/24_hydration_bugs`: an effect runs only in the
    // browser, once hydrated, so the server and the hydration render alike.
    let hydrated = RwSignal::new(false);
    Effect::new(move |_| hydrated.set(true));
    let asked = RwSignal::new(None::<(Screen, Slot)>);
    let focus = StoredValue::new(None::<usize>);
    // NOTE: WCAG 2.2 SC 2.4.3 and 3.3.1: focus follows an answer to its card, and
    // a refused field is marked; a change's answer waits for the page drawn again.
    Effect::new(move |_| {
        value.with(|answer| match answer {
            Some(Ok(answer)) if answer.changed => focus.set_value(Some(version.get_untracked())),
            Some(Ok(answer)) => {
                focus.set_value(Some(changes.get_untracked()));
                browser::marked(answer.field.as_ref());
            }
            Some(Err(_)) => focus.set_value(Some(changes.get_untracked())),
            None => {}
        });
    });
    let page = Page {
        configure,
        hydrated,
        asked,
        screen,
        focus,
    };
    view! {
        <Title text=crate::views::titled(&tr(Msg::Configuration)) />
        <Transition fallback=|| {
            view! { <p>{tr(Msg::LoadingTheConfiguration)}</p> }
        }>
            {move || Suspend::new(async move {
                let loaded = loaded.await;
                let drawn = changes.get_untracked();
                crate::views::shown(loaded, move |view| config_section(view, page, drawn))
            })}
        </Transition>
    }
}

/// What every part of the page shares.
#[derive(Clone, Copy)]
struct Page {
    /// The server function every form posts to.
    configure: ServerAction<Configure>,
    /// Whether the page has hydrated, so its forms run.
    hydrated: RwSignal<bool>,
    /// Where the last form was sent from: the part of the page and its card.
    asked: RwSignal<Option<(Screen, Slot)>>,
    /// The part of the page the path names.
    screen: Memo<Screen>,
    /// The drawing of the page, by the changes it shows, whose outcome takes
    /// the focus once it is drawn; `None` once it has.
    focus: StoredValue<Option<usize>>,
}

/// The outcome of the last form, shown in the card `slot` when the form was
/// sent from it on this part of the page: the answer the server rendered,
/// or the console's fault, focusable so the focus can move to it.
fn outcome(page: Page, slot: Slot, drawn: usize) -> impl IntoView {
    let Page {
        configure,
        asked,
        screen,
        focus,
        ..
    } = page;
    let (value, pending) = (configure.value(), configure.pending());
    let shown = NodeRef::<leptos::html::Div>::new();
    // NOTE: WCAG 2.2 SC 2.4.3: focus follows an answer to its card once the card
    // that shows it is drawn, also when a change has drawn the page again.
    Effect::new(move |_| {
        if let Some(shown) = shown.get()
            && focus.get_value() == Some(drawn)
        {
            focus.set_value(None);
            match shown.focus() {
                Ok(()) | Err(_) => {}
            }
        }
    });
    move || {
        let here = asked.with(|asked| {
            asked
                .as_ref()
                .is_some_and(|(at, sent)| *sent == slot && screen.with(|now| now == at))
        });
        if !here {
            return None;
        }
        if pending.get() {
            return Some(busy(tr(Msg::SendingTheChange), Busy::Inline));
        }
        value.with(|answer| match answer {
            None => None,
            Some(Ok(rendered)) => {
                let html = rendered.html.clone();
                Some(
                    view! {
                        <div
                            id="config-outcome"
                            node_ref=shown
                            tabindex="-1"
                            class="fui-well"
                            inner_html=html
                        ></div>
                    }
                    .into_any(),
                )
            }
            Some(Err(error)) => Some(
                view! {
                    <div id="config-outcome" node_ref=shown tabindex="-1">
                        {crate::views::fault(error)}
                    </div>
                }
                .into_any(),
            ),
        })
    }
}

/// The pending card, when the server drew one, with the outcome of a form
/// sent from it at its top.
fn pending_card(html: String, page: Page, drawn: usize) -> AnyView {
    if html.is_empty() {
        return ().into_any();
    }
    view! {
        <section class="fui-card">
            {outcome(page, Slot::Pending, drawn)}
            <div inner_html=html class="fui-stack fui-gap-default"></div>
        </section>
    }
    .into_any()
}

/// The page the server rendered, its forms in one fieldset that hands the
/// form submitted to the server function once the page has hydrated.
fn config_section(view: ConfigView, page: Page, drawn: usize) -> AnyView {
    let Page {
        configure,
        hydrated,
        asked,
        screen,
        ..
    } = page;
    // NOTE: no specification governs this: our own design; the countdown of a
    // confirmation runs while the page that shows it does.
    Effect::new(move |_| {
        browser::countdown();
    });
    let submit = move |event: leptos::ev::SubmitEvent| {
        // NOTE: no specification governs this: our own design; form data the
        // browser cannot give leaves the plain post, which the server refuses.
        if let Some((form, pending)) = browser::submitted(&event) {
            event.prevent_default();
            let slot = if pending { Slot::Pending } else { Slot::Main };
            asked.set(Some((screen.get_untracked(), slot)));
            configure.dispatch(Configure { form });
        }
    };
    let ConfigView {
        header,
        pending,
        tabs,
        tab,
        lead,
        main,
    } = view;
    let busy = configure.pending();
    view! {
        <div inner_html=header></div>
        <fieldset
            class="fui-fieldset fui-stack"
            disabled=move || !hydrated.get()
            aria-busy=move || busy.get().to_string()
            on:submit=submit
            on:click=move |event| browser::clicked(&event)
            on:keydown=move |event| browser::keyed(&event)
            on:input=move |event| browser::typed(&event)
        >
            <legend class="fui-sr-only">{tr(Msg::ConfigurationChanges)}</legend>
            <p role="status" class="fui-help">
                {move || crate::admin::ready(hydrated.get())}
            </p>
            {pending_card(pending, page, drawn)}
            <div inner_html=tabs></div>
            <div id="config-panel" role="tabpanel" aria-labelledby=tab class="fui-stack">
                <div inner_html=lead></div>
                <div class="fui-card">
                    {outcome(page, Slot::Main, drawn)}
                    <div inner_html=main class="fui-stack fui-gap-default"></div>
                </div>
            </div>
        </fieldset>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::countdown;

    #[test]
    fn the_countdown_reads_as_minutes_and_seconds_and_says_when_it_has_passed() {
        assert_eq!("1:43 left", countdown(103_400));
        assert_eq!("0:05 left", countdown(5_999));
        assert_eq!("The deadline has passed", countdown(0));
        assert_eq!("The deadline has passed", countdown(-3));
    }
}
