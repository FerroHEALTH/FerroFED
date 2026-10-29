// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The configuration page (T3), drawn from the first replica's admin
//! listener.
//!
//! Its binder tabs are the settings of the running configuration
//! (Federation behaviour, Fan-out and paging, Consent, Stored queries,
//! Access log and FHIR face), then Members, Credentials and bindings,
//! History, and Import and export, each its own route; a form's page sits
//! under its tab. With configuration revisions off, the settings tabs show
//! alone, read-only. A post of one of its forms runs through the
//! configuration's server function, and its answer shows at the top. A
//! console configured with no
//! admin listener reads the gateway but offers no configuration, and says
//! how to add one.

use ferrohealth_ui::view::surface::{Header, page_heading, stack};
use ferrohealth_ui::view::tabs::{Tab as BinderTab, binder};
use ferrohealth_ui::widget::action::answered;
use leptos::prelude::*;

use crate::act::configure::{ConfigAnswer, Configure};
use crate::configure::model::{ConfigView, Screen};
use crate::configure::render::Tab;
use crate::configure::said::{ActionAnswer, Answer};
use crate::locale::{Msg, tr};
use crate::pages::stopped;

/// The path of the form that adds a member: an organisation with its first
/// node and that node's first endpoint.
pub const NEW_MEMBER: &str = "/configuration/members/new";

/// The path of the form that stores a new stored-query version.
pub const NEW_QUERY: &str = "/configuration/stored-queries/new";

/// Whether the console can change the configuration: it is configured
/// with a replica's admin listener.
#[must_use]
pub fn console_manages() -> bool {
    !crate::visit::visit_untracked().replicas().is_empty()
}

/// The path of the form that adds a node to the organisation `org`.
#[must_use]
pub fn new_node(org: &str) -> String {
    format!("{NEW_MEMBER}?organisation={org}")
}

/// The path of the form that edits the member of the node `node`.
#[must_use]
pub fn edit_member(node: &str) -> String {
    format!("/configuration/members/{node}")
}

/// The configuration, on the part the path names.
#[must_use]
pub fn page() -> AnyView {
    if !console_manages() {
        return no_admin();
    }
    let screen = Screen {
        tab: crate::pages::param("tab"),
        item: crate::pages::param("item"),
        compare: crate::pages::query_param("compare").unwrap_or_default(),
    };
    let configure = ServerAction::<Configure>::new();
    // NOTE: no specification governs this: our own design; the answer carries the form as
    // the server read it, so the page draws a form again from the answer.
    let _posted = answered(&configure);
    crate::read::follow(configure);
    provide_context(configure);
    let asked = screen.clone();
    let read = Resource::new(crate::read::changes, move |_| {
        crate::act::configure::config_now(asked.clone())
    });
    view! {
        {crate::app::title(tr(Msg::Configuration))}
        <Suspense fallback=|| ()>
            {crate::pages::afresh(
                move || configure.version().get(),
                move || {
                    let screen = screen.clone();
                    let ConfigAnswer { answer, draft } = match configure.value().get() {
                        Some(Ok(answered)) => answered,
                        Some(Err(stop)) => {
                            ConfigAnswer {
                                answer: Some(ActionAnswer {
                                    answer: match stop {
                                        crate::read::Stop::Refused(refusal) => {
                                            Answer::Refused(refusal)
                                        }
                                        crate::read::Stop::Fault(fault) => Answer::Failed(fault),
                                    },
                                    changed: false,
                                    field: None,
                                }),
                                draft: None,
                            }
                        }
                        None => ConfigAnswer::default(),
                    };
                    Suspend::new(async move {
                        if let Some(draft) = &draft {
                            provide_context(draft.clone());
                            if let Some(note) = draft.field.clone() {
                                provide_context(note);
                            }
                        }
                        let answer = answer.map(|answer| answer.view());
                        match read
                            .await
                            .and_then(|data| {
                                data.view(&screen, draft.as_ref())
                                    .map_err(crate::read::Stop::Refused)
                            })
                        {
                            Ok(view) => drawn(view, answer),
                            Err(stop) => {
                                view! {
                                    {heading()}
                                    {answer}
                                    {stopped(&stop)}
                                }
                                    .into_any()
                            }
                        }
                    })
                },
            )}
        </Suspense>
    }
    .into_any()
}

/// The page's plain heading.
fn heading() -> AnyView {
    page_heading(
        Header {
            title: tr(Msg::Configuration).into(),
            ..Header::default()
        },
        ferrohealth_ui::view::action::ActionGroup::none(),
    )
    .into_any()
}

/// The page of `view`, with a form's `answer` at the top.
fn drawn(view: ConfigView, answer: Option<AnyView>) -> AnyView {
    let ConfigView {
        header,
        pending,
        tab,
        lead,
        main,
        file_only,
    } = view;
    let shown: &[Tab] = if file_only { &Tab::SETTINGS } else { &Tab::ALL };
    let section = match tab {
        Some(tab) => {
            let tabs = shown
                .iter()
                .map(|each| BinderTab::new(each.id(), each.href(), tr(each.label())))
                .collect();
            binder(
                tr(Msg::Configuration),
                tabs,
                tab.id(),
                view! { <div class="fui-stack fui-panel-body">{lead} {main}</div> }.into_any(),
            )
        }
        None => main.unwrap_or_else(|| ().into_any()),
    };
    view! {
        {header}
        {stack(
            view! {
                {answer.map(|answer| view! { <div id="config-outcome">{answer}</div> })}
                {pending}
                {section}
            },
        )}
    }
    .into_any()
}

/// The page of a console configured with no admin listener: it reads the
/// gateway but offers no configuration, and says how to add one.
fn no_admin() -> AnyView {
    view! {
        {crate::app::title(tr(Msg::Configuration))}
        {heading()}
        <section class="fui-card">
            <p>{tr(Msg::ConfigNoAdminBody)}</p>
            <p class="fui-small fui-muted">{tr(Msg::ConfigNoAdminHow)}</p>
        </section>
    }
    .into_any()
}
