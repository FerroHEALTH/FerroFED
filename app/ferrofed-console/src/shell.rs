// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The chrome around every page a signed-in operator reads, drawn with
//! FerroUI's shell.
//!
//! The top bar holds the lockup, the find over organisations, nodes and
//! stored queries, the gateway's status pill and the operator's menu, which
//! holds the theme (the top bar carries no theme control, family rule 5).
//! The navigation is grouped as the design draws it, with the open
//! incidents counted on Integrity (R12) and, inside an organisation, the
//! organisation and its nodes nested under Organisations (F10); it folds to
//! a rail from 48 rem up and is a drawer below. The development banner
//! sits under the top bar while the gateway runs the development profile.
//! The shell is drawn once and survives a navigation in the page; the
//! drawer, the menu and the find work with no WebAssembly too.
#![cfg_attr(
    target_arch = "wasm32",
    allow(
        clippy::unused_async_trait_impl,
        reason = "the server function macro writes the browser half's call as an async trait method; only that target has it"
    )
)]

use ferrohealth_ui::i18n::Arg;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::search::{SearchBox, search_box};
use ferrohealth_ui::view::shell::{
    Footer, Shell, TopBar, brand_mark, lockup, shell as shell_view, topbar,
};
use ferrohealth_ui::view::status::{Reach, StatusPill, banner, status_pill};
use ferrohealth_ui::widget::menu::{ThemeChoice, UserMenu, user_menu};
use ferrohealth_ui::widget::nav::{Count, Entry, Group, Nest, NestItem, Rail, Sidebar, sidebar};
use leptos::prelude::*;
use leptos_router::hooks::use_location;
use serde::{Deserialize, Serialize};

use crate::app::{
    ABOUT, ALL_NODES, CONFIGURATION, INTEGRITY, MARK_DARK, MARK_LIGHT, ORGANISATIONS, PRODUCT,
    QUERY, SEARCH, SELF_DESCRIPTION, SERVICES, SIGN_OUT, STORED_QUERIES, TOPOLOGY, VERSION,
};
use crate::locale::{Msg, tr, tr_with};
use crate::read::federation::{Federation, Health};
use crate::read::{Deployment, Fault, Refusal, Stop};

/// The id of the navigation, which the drawer button opens.
pub const SIDEBAR_ID: &str = "sidebar";

/// The id of the operator's menu.
pub const USER_MENU_ID: &str = "user-menu";

/// The id of the find in the top bar.
pub const SEARCH_ID: &str = "find";

/// The footer every page carries.
#[must_use]
pub fn footer() -> Footer {
    Footer {
        product: PRODUCT.into(),
        version: VERSION.into(),
        about: ABOUT.into(),
    }
}

/// The lockup: the brand mark, FerroFED and the word Console, leading to
/// the dashboard.
#[must_use]
pub fn brand_lockup() -> AnyView {
    lockup(
        "/",
        brand_mark(MARK_LIGHT, MARK_DARK),
        "FerroFED",
        tr(Msg::LockupWord),
    )
    .into_any()
}

/// What the shell reads of the gateway for one page: how it is deployed,
/// the open incidents, and the federation where the page sits inside an
/// organisation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reads {
    deployment: Result<Deployment, Stop>,
    /// The gateway's status from its self-description, read when its
    /// deployment cannot be: the federation and the specification version.
    described: Option<Result<(String, String), Stop>>,
    open_incidents: Option<u32>,
    federation: Option<Federation>,
}

/// What the shell of a page reads, with the federation when `nested`, the
/// page sitting inside an organisation, as the operator of the request.
///
/// # Errors
/// Returns [`crate::read::Fault::Unavailable`] when the console's server
/// does not answer.
#[server(endpoint = "shell")]
pub async fn reads_now(
    /// Whether the page sits inside an organisation.
    nested: bool,
) -> Result<Reads, Stop> {
    Ok(reads(nested).await)
}

/// Reads what the shell of a page draws.
#[cfg(not(target_arch = "wasm32"))]
async fn reads(nested: bool) -> Reads {
    let deployment = crate::read::deployment().await;
    // NOTE: no specification governs this: our own design; an operator whose token
    // reads no deployment still reads the gateway as up when it describes itself.
    let described = match &deployment {
        Ok(_) | Err(Stop::Refused(Refusal::NotAuthenticated { .. })) => None,
        Err(_) => Some(crate::read::described().await),
    };
    let reachable = deployment.is_ok() || matches!(described, Some(Ok(_)));
    let open_incidents = if reachable {
        crate::pages::integrity::open_count().await
    } else {
        None
    };
    let federation = if nested && reachable {
        crate::read::federation::read().await.ok()
    } else {
        None
    };
    Reads {
        deployment,
        described,
        open_incidents,
        federation,
    }
}

/// The organisation id a page at `path` sits inside, for the nested
/// navigation.
fn nested_org(path: &str) -> Option<&str> {
    let rest = path.strip_prefix(&format!("{ORGANISATIONS}/"))?;
    let org = rest.split('/').next()?;
    (!org.is_empty() && org != "nodes" && org != "new").then_some(org)
}

/// The page `content` in the shell of a signed-in operator: the top bar,
/// the banner and the navigation each read the gateway as they are drawn,
/// and the page itself is drawn once inside them.
#[must_use]
pub fn shell(content: AnyView) -> AnyView {
    // NOTE: FerroUI `docs/consoles.md`, "How a console builds", step 10: the shell
    // is drawn once, and the navigation and the `back` of its forms follow the router.
    ferrohealth_ui::widget::nav::follow_path();
    let location = use_location();
    let nested = Memo::new(move |_| nested_org(&location.pathname.get()).is_some());
    let reads = Resource::new(
        move || (crate::read::changes(), nested.get()),
        |(_, nested)| reads_now(nested),
    );
    let read = move || async move {
        reads.await.unwrap_or_else(|stop| Reads {
            deployment: Err(stop),
            described: None,
            open_incidents: None,
            federation: None,
        })
    };
    let bar = view! {
        <Suspense>
            {move || {
                let path = location.pathname.get_untracked();
                let query = location.search.get_untracked();
                let searched = (path == SEARCH)
                    .then(|| crate::pages::search::query_of(&query))
                    .flatten();
                let visit = crate::visit::visit();
                Suspend::new(async move { top_bar(&path, &read().await, searched, &visit) })
            }}
        </Suspense>
    }
    .into_any();
    let banner = view! { <Suspense>{move || Suspend::new(async move { development(&read().await) })}</Suspense> }
    .into_any();
    let nav = view! {
        <Suspense>
            {move || {
                let path = location.pathname.get();
                let rail = crate::visit::visit().chosen().rail;
                let replicas = crate::visit::visit().replicas();
                Suspend::new(async move { navigation_of(&path, rail, &replicas, &read().await) })
            }}
        </Suspense>
    }
    .into_any();
    shell_view(Shell {
        topbar: bar,
        banner: Some(banner),
        sidebar: nav,
        content,
        dock: None,
        footer: footer(),
    })
}

/// The top bar of the shell drawn at `path` from `reads`, with the find's
/// `searched` words, in the visit `visit`.
fn top_bar(
    path: &str,
    reads: &Reads,
    searched: Option<String>,
    visit: &crate::visit::Visit,
) -> AnyView {
    let reachable = reads.deployment.is_ok() || matches!(reads.described, Some(Ok(_)));
    let search = search_box(find(searched, reachable));
    let pill = status_pill(top_pill(reads)).into_any();
    let operator = visit.operator.clone().unwrap_or_else(|| tr(Msg::Operator));
    let menu = user_menu(UserMenu {
        id: USER_MENU_ID,
        label: operator.clone(),
        name: Some(operator.clone()),
        who: operator,
        line: tr(Msg::OperatorLine),
        preferences: String::from(crate::prefs::PREFERENCES),
        theme: ThemeChoice {
            current: visit.chosen().preferences.theme,
            action: String::from(crate::prefs::PREFERENCE),
            back: path.to_owned(),
        },
        choices: Vec::new(),
        shortcuts: None,
        sign_out: Some(String::from(SIGN_OUT)),
        links: Vec::new(),
    });
    let end = view! {
        {pill}
        {menu}
    }
    .into_any();
    topbar(TopBar {
        sidebar: SIDEBAR_ID,
        lockup: brand_lockup(),
        search: Some(search),
        end,
    })
}

/// The top bar's find, holding the `searched` words, disabled with its
/// reason when the gateway is not `reachable`; once the page has hydrated
/// its panel asks [`crate::pages::search::ANSWER`] as the words are typed.
fn find(searched: Option<String>, reachable: bool) -> SearchBox {
    SearchBox {
        id: String::from(SEARCH_ID),
        name: String::from("q"),
        label: tr(Msg::SearchLabel),
        placeholder: tr(Msg::SearchPlaceholder),
        action: String::from(SEARCH),
        value: searched.unwrap_or_default(),
        submit: tr(Msg::SearchSubmit).into(),
        shortcut: Some('/'),
        busy: false,
        disabled: (!reachable).then(|| tr(Msg::SearchDisabled)),
        post: false,
        suggest: Some(String::from(crate::pages::search::ANSWER)),
        suggest_from: 2,
        all: None,
        signed_out: Some(String::from(crate::app::SIGN_IN)),
    }
}

/// The development banner, while the gateway runs the development profile.
fn development(reads: &Reads) -> AnyView {
    let development = reads
        .deployment
        .as_ref()
        .is_ok_and(|deployment| deployment.development);
    development
        .then(|| {
            banner(
                Tone::Warn,
                Glyph::TRIANGLE_ALERT,
                tr(Msg::DevelopmentTitle),
                &tr(Msg::DevelopmentBody),
                None,
            )
        })
        .into_any()
}

/// The navigation of the shell drawn at `path`, folded to its rail when
/// `rail`, with the configuration where the console reaches `replicas`,
/// from `reads`.
fn navigation_of(path: &str, rail: bool, replicas: &[String], reads: &Reads) -> AnyView {
    let reachable = reads.deployment.is_ok() || matches!(reads.described, Some(Ok(_)));
    sidebar(Sidebar {
        id: SIDEBAR_ID,
        label: tr(Msg::NavLabel),
        groups: navigation(reads.open_incidents, reachable, !replicas.is_empty()),
        current: path.to_owned(),
        rail: Some(Rail {
            collapsed: rail,
            action: String::from(crate::prefs::SIDEBAR),
            back: path.to_owned(),
        }),
        nest: reads
            .federation
            .as_ref()
            .and_then(|federation| nest(federation, path)),
    })
}

/// The status pill of the gateway: its deployment's, or, when the
/// deployment cannot be read, its self-description's.
fn top_pill(reads: &Reads) -> StatusPill {
    match (&reads.deployment, &reads.described) {
        (Err(_), Some(Ok((federation, spec)))) => StatusPill {
            reach: Reach::Up,
            words: tr(Msg::GatewayUp),
            detail: Some(tr_with(
                Msg::GatewayDetailSpec,
                &[
                    ("federation", federation.clone().into()),
                    ("spec", spec.clone().into()),
                ],
            )),
        },
        (Err(_), Some(Err(stop))) => pill(&Err(stop.clone())),
        (deployment, _) => pill(deployment),
    }
}

/// The status pill of the gateway the console reached, or of why it could
/// not.
fn pill(deployment: &Result<Deployment, Stop>) -> StatusPill {
    match deployment {
        Ok(deployment) => StatusPill {
            reach: Reach::Up,
            words: tr(Msg::GatewayUp),
            detail: Some(match &deployment.replica {
                Some(replica) => tr_with(
                    Msg::GatewayDetailReplica,
                    &[
                        ("version", deployment.version.clone().into()),
                        ("replica", replica.clone().into()),
                    ],
                ),
                None => tr_with(
                    Msg::GatewayDetail,
                    &[("version", deployment.version.clone().into())],
                ),
            }),
        },
        Err(Stop::Refused(Refusal::NotAuthenticated { .. })) => StatusPill {
            reach: Reach::Refused,
            words: tr(Msg::GatewayRefused401),
            detail: None,
        },
        Err(Stop::Refused(Refusal::Gateway { status, .. })) => StatusPill {
            reach: Reach::Refused,
            words: tr_with(
                Msg::GatewayRefused,
                &[("status", Arg::from(u32::from(*status)))],
            ),
            detail: None,
        },
        Err(Stop::Refused(Refusal::Invalid { .. }) | Stop::Fault(Fault::Unreadable { .. })) => {
            StatusPill {
                reach: Reach::Unknown,
                words: tr(Msg::GatewayUnknown),
                detail: None,
            }
        }
        Err(Stop::Fault(_)) => StatusPill {
            reach: Reach::Unreachable,
            words: tr(Msg::GatewayUnreachable),
            detail: None,
        },
    }
}

/// One entry of the navigation.
fn entry(href: &'static str, label: Msg, glyph: Glyph, exact: bool) -> Entry {
    Entry {
        href: href.into(),
        label: tr(label).into(),
        glyph,
        exact,
        count: None,
    }
}

/// The navigation's groups, Integrity counting the `open` incidents, or a
/// dash with the reason when the gateway did not answer.
fn navigation(open: Option<u32>, reachable: bool, manages: bool) -> Vec<Group> {
    let mut integrity = entry(INTEGRITY, Msg::NavIntegrity, Glyph::SHIELD_CHECK, false);
    integrity.count = Some(match open {
        Some(count) => Count::Known {
            count,
            words: tr_with(Msg::NavOpenIncidents, &[("count", Arg::from(count))]),
        },
        None => Count::Unknown {
            why: tr(if reachable {
                Msg::NavIncidentsUnread
            } else {
                Msg::NavIncidentsUnknown
            }),
        },
    });
    vec![
        Group {
            id: "nav-overview",
            title: tr(Msg::NavGroupOverview).into(),
            entries: vec![
                entry("/", Msg::NavDashboard, Glyph::LAYOUT_DASHBOARD, true),
                entry(TOPOLOGY, Msg::NavTopology, Glyph::NETWORK, false),
            ],
        },
        Group {
            id: "nav-federation",
            title: tr(Msg::NavGroupFederation).into(),
            entries: vec![
                entry(
                    ORGANISATIONS,
                    Msg::NavOrganisations,
                    Glyph::BUILDING_COMPLEX,
                    false,
                ),
                entry(SERVICES, Msg::NavServices, Glyph::PLUG, false),
                integrity,
            ],
        },
        Group {
            id: "nav-queries",
            title: tr(Msg::NavGroupQueries).into(),
            entries: vec![
                entry(QUERY, Msg::NavQuery, Glyph::SQUARE_TERMINAL, false),
                entry(STORED_QUERIES, Msg::NavStored, Glyph::DATABASE, false),
            ],
        },
        Group {
            id: "nav-gateway",
            title: tr(Msg::NavGroupGateway).into(),
            entries: vec![entry(
                SELF_DESCRIPTION,
                Msg::NavSelfDescription,
                Glyph::SERVER,
                false,
            )],
        },
        Group {
            id: "nav-console",
            title: tr(Msg::NavGroupConsole).into(),
            // NOTE: no specification governs this: our own design; a console with no
            // admin listener changes no configuration, so it offers no entry for it.
            entries: manages
                .then(|| {
                    entry(
                        CONFIGURATION,
                        Msg::NavConfiguration,
                        Glyph::FILE_TEXT,
                        false,
                    )
                })
                .into_iter()
                .chain([entry(ABOUT, Msg::NavAbout, Glyph::INFO, true)])
                .collect(),
        },
    ]
}

/// The organisation the page at `path` sits inside, with its nodes, nested
/// under Organisations (F10).
fn nest(federation: &Federation, path: &str) -> Option<Nest> {
    nested_org(path)?;
    let organisation = federation.organisations.iter().find(|organisation| {
        let href = organisation.href();
        path == href || path.starts_with(&format!("{href}/"))
    })?;
    let children: Vec<NestItem> = organisation
        .nodes
        .iter()
        .filter(|node| node.routable())
        .map(|node| NestItem {
            label: node.id.clone(),
            href: organisation.node_href(node),
            state: node.health().flow(),
            state_words: node.health().word(),
            mono: true,
        })
        .collect();
    let count = u32::try_from(children.len()).unwrap_or(u32::MAX);
    let worst = organisation.health();
    Some(Nest {
        under: ORGANISATIONS.into(),
        label: tr_with(
            Msg::NestLabel,
            &[("organisation", organisation.shown().to_owned().into())],
        ),
        parent: NestItem {
            label: organisation.shown().to_owned(),
            href: organisation.href(),
            state: worst.flow(),
            state_words: tr_with(
                Msg::NestWorst,
                &[("state", worst.word().to_lowercase().into())],
            ),
            mono: false,
        },
        count_words: tr_with(Msg::NodeCount, &[("count", Arg::from(count))]),
        children,
        all: tr_with(Msg::NestAll, &[("count", Arg::from(count))]),
    })
}

/// Whether the health of a whole federation asks for the operator's
/// attention, for the dashboard's cards.
#[must_use]
pub fn attention(health: Health) -> bool {
    health.is_problem()
}

/// The `href` of every node across the organisations, which the shell's
/// links and the dashboard share.
#[must_use]
pub const fn all_nodes() -> &'static str {
    ALL_NODES
}

#[cfg(test)]
mod tests {
    use super::find;

    #[test]
    fn the_find_names_its_answer_route_and_submits_to_the_find_page() {
        let find = find(Some(String::from("node-b")), true);
        assert_eq!(Some(crate::pages::search::ANSWER), find.suggest.as_deref());
        assert_eq!(crate::app::SEARCH, find.action);
        assert_eq!("node-b", find.value);
        assert!(find.disabled.is_none());
    }
}
