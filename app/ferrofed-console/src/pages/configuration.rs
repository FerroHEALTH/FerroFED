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
//! alone, read-only. Each form posts to the page it sits on, which the
//! server draws again with its answer at the top. A console configured with
//! no admin listener reads the gateway but offers no configuration, and
//! says how to add one.

use ferrohealth_ui::html::Html;
use ferrohealth_ui::view::surface::{Header, page_heading, stack};
use ferrohealth_ui::view::tabs::{Tab as BinderTab, binder};

use crate::act::configure::ConfigAnswer;
use crate::act::fields::Fields;
use crate::configure::model::{ConfigView, Screen};
use crate::configure::render::Tab;
use crate::configure::said::{ActionAnswer, Answer};
use crate::locale::{Msg, current, tr};
use crate::pages::{h, opt, stopped, t};

/// The path of the form that adds a member: an organisation with its first
/// node and that node's first endpoint.
pub const NEW_MEMBER: &str = "/configuration/members/new";

/// The path of the form that stores a new stored-query version.
pub const NEW_QUERY: &str = "/configuration/stored-queries/new";

/// Whether the console can change the configuration: it is configured
/// with a replica's admin listener.
#[must_use]
pub fn console_manages() -> bool {
    crate::visit::with(|visit| !visit.replicas().is_empty())
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

/// The configuration, on the part the path names, with the answer of the
/// form `posted` to it at the top.
pub async fn page(posted: Option<Fields>) -> Html {
    if !console_manages() {
        return no_admin();
    }
    let screen = Screen {
        tab: crate::pages::param("tab"),
        item: crate::pages::param("item"),
        compare: crate::pages::query_param("compare").unwrap_or_default(),
    };
    // NOTE: no specification governs this: our own design; the form runs before the page
    // reads the replica, so the page shows what the run changed.
    let ConfigAnswer { answer, draft } = match posted {
        Some(fields) => match crate::act::configure::configure(&fields).await {
            Ok(answered) => answered,
            Err(stop) => ConfigAnswer {
                answer: Some(ActionAnswer {
                    answer: match stop {
                        crate::read::Stop::Refused(refusal) => Answer::Refused(refusal),
                        crate::read::Stop::Fault(fault) => Answer::Failed(fault),
                    },
                    changed: false,
                    field: None,
                }),
                draft: None,
            },
        },
        None => ConfigAnswer::default(),
    };
    let read = crate::act::configure::config_now(screen.clone()).await;
    let body = crate::configure::render::parts::drawing(draft.clone(), || {
        let answer = answer.map(|answer| answer.view());
        match read.and_then(|data| {
            data.view(&screen, draft.as_ref())
                .map_err(crate::read::Stop::Refused)
        }) {
            Ok(view) => drawn(view, answer),
            Err(stop) => Html::join([heading(), opt(answer), stopped(&stop)]),
        }
    });
    crate::app::title(tr(Msg::Configuration)).then(body)
}

/// The page's plain heading.
fn heading() -> Html {
    page_heading(
        &current(),
        Header {
            title: tr(Msg::Configuration).into(),
            ..Header::default()
        },
        ferrohealth_ui::view::action::ActionGroup::none(),
    )
}

/// The page of `view`, with a form's `answer` at the top.
fn drawn(view: ConfigView, answer: Option<Html>) -> Html {
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
                &current(),
                tr(Msg::Configuration),
                tabs,
                tab.id(),
                h(format!(
                    "<div class=\"fui-stack fui-panel-body\">{} {}</div>",
                    opt(lead),
                    opt(main)
                )),
            )
        }
        None => opt(main),
    };
    let answer = answer.map(|answer| h(format!("<div id=\"config-outcome\">{answer}</div>")));
    opt(header).then(stack(Html::join([opt(answer), opt(pending), section])))
}

/// The page of a console configured with no admin listener: it reads the
/// gateway but offers no configuration, and says how to add one.
fn no_admin() -> Html {
    Html::join([
        crate::app::title(tr(Msg::Configuration)),
        heading(),
        h(format!(
            "<section class=\"fui-card\"><p>{}</p><p class=\"fui-small fui-muted\">{}</p></section>",
            t(tr(Msg::ConfigNoAdminBody)),
            t(tr(Msg::ConfigNoAdminHow))
        )),
    ])
}
