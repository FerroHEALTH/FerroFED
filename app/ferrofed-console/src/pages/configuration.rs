// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The configuration page (T3), drawn from the first replica's admin
//! listener.
//!
//! Its binder tabs are Members, Credentials and bindings, Stored queries,
//! History, and Import and export, each its own route; a form's page sits
//! under its tab. A post of one of its forms is run while the page is drawn
//! again, and its answer shows at the top. A console configured with no
//! admin listener reads the gateway but offers no configuration, and says
//! how to add one.

use ferrohealth_ui::view::surface::{Header, page_heading, stack};
use ferrohealth_ui::view::tabs::{Tab as BinderTab, binder};
use leptos::prelude::*;

use crate::act::configure::ConfigPost;
use crate::configure::draft::{Draft, Edit};
use crate::configure::model::{ConfigView, Screen};
use crate::configure::render::Tab;
use crate::configure::said::ActionAnswer;
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
    use_context::<crate::server::ConsoleState>()
        .is_some_and(|state| !state.settings().admin.is_empty())
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
    let posted = use_context::<ConfigPost>();
    let body = Suspend::new(async move {
        let (state, token) = match crate::read::signed_in() {
            Ok(signed_in) => signed_in,
            Err(fault) => return stopped(&crate::read::Stop::Fault(fault)),
        };
        let Some(replica) = state.settings().admin.first() else {
            return no_admin();
        };
        let (answer, draft) = match posted {
            Some(ConfigPost(form)) => answered(state.gateway(), replica, &token, form).await,
            None => (None, None),
        };
        if let Some(draft) = &draft {
            provide_context(draft.clone());
            if let Some(note) = draft.field.clone() {
                provide_context(note);
            }
        }
        let view =
            crate::configure::act::page(state.gateway(), replica, &token, &screen, draft.as_ref())
                .await;
        match view {
            Ok(view) => drawn(view, answer.map(|answer| answer.view)),
            Err(stop) => view! {
                {heading()}
                {answer.map(|answer| answer.view)}
                {stopped(&stop)}
            }
            .into_any(),
        }
    });
    view! {
        {crate::app::title(tr(Msg::Configuration))}
        <Suspense fallback=|| ()>{body}</Suspense>
    }
    .into_any()
}

/// What a post of `form` gives: the answer at the top of the page, and the
/// form to draw again, as posted, when it changed nothing. An Add or a
/// Remove of a row runs nothing and answers nothing.
async fn answered(
    gateway: &crate::gateway::Gateway,
    replica: &crate::config::settings::ReplicaSettings,
    token: &crate::gateway::AccessToken,
    mut form: crate::configure::model::ConfigForm,
) -> (Option<ActionAnswer>, Option<Draft>) {
    if let Some(edit) = Edit::of(&form.edit) {
        edit.apply(&mut form);
        return (None, Some(Draft { form, field: None }));
    }
    let answer = match crate::configure::plan::plan(&mut form) {
        Ok(plan) => crate::configure::act::run(gateway, replica, token, plan).await,
        Err(refused) => crate::configure::render::answers::refused(&refused),
    };
    let draft = (!answer.changed).then(|| Draft {
        form,
        field: answer.field.clone(),
    });
    (Some(answer), draft)
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
    } = view;
    let section = match tab {
        Some(tab) => {
            let tabs = Tab::ALL
                .into_iter()
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
