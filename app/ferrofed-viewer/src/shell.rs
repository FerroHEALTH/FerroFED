// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The chrome around every page.
//!
//! It is the top bar with the gateway's status, the theme switch and the
//! operator's menu; the grouped navigation, a column beside the page from
//! `md` and a drawer below it; and the one-line footer.
//!
//! The drawer and the operator's menu are HTML popovers the top bar's
//! buttons open, so the browser opens them, closes them on Escape and
//! returns the focus to the button, before the page has hydrated and with
//! no script (HTML, the `popover` attribute). The browser half adds what
//! HTML leaves out: the focus moves into the drawer as it opens, and the
//! drawer closes when one of its links is followed.

use leptos::prelude::*;
use leptos::server_fn::ServerFn as _;
use leptos_router::components::A;

use crate::app::{ABOUT, PRODUCT, SIGN_OUT, VERSION};
use crate::components::icon;
use crate::manufacturer::MANUFACTURER;
use crate::styles;
use crate::theme::{SetTheme, Theme};
use crate::views::model::{GatewayView, Outcome, Refusal, ViewError};

/// The id of the navigation, which the drawer button opens.
const SIDEBAR_ID: &str = "sidebar";

/// The id of the operator's menu, which the operator's button opens.
const MENU_ID: &str = "operator-menu";

/// What the top bar of a signed-in page shows.
#[derive(Debug, Clone)]
pub struct Bar {
    /// The operator's name, as the provider gave it.
    pub operator: String,
    /// The theme in force, which the switch marks and changes.
    pub theme: RwSignal<Theme>,
    /// The gateway's status, loaded once for the page.
    pub gateway: Resource<Result<Outcome<GatewayView>, ViewError>>,
}

/// The top bar of a signed-in page: the drawer button, the lockup, and at
/// the far end the gateway's status, the theme switch and the operator.
#[must_use]
pub fn topbar(bar: Bar) -> AnyView {
    let gateway = bar.gateway;
    let status = view! {
        <Suspense fallback=|| {
            view! { <span class=styles::Tone::Neutral.pill()>"Gateway"</span> }
        }>{move || Suspend::new(async move { gateway_pill(gateway.await) })}</Suspense>
    };
    view! {
        <header class=styles::TOPBAR>
            <div class=styles::TOPBAR_ROW>
                <button
                    type="button"
                    class=styles::DRAWER_BUTTON
                    popovertarget=SIDEBAR_ID
                    aria-controls=SIDEBAR_ID
                >
                    {icon(icondata_lu::LuMenu, "16")}
                    "Menu"
                </button>
                {lockup()}
                <div class=styles::TOPBAR_END>
                    {status} <div class="hidden sm:block">{theme_switch(bar.theme)}</div>
                    {operator_menu(bar.operator, bar.theme)}
                </div>
            </div>
        </header>
    }
    .into_any()
}

/// The mark and the product name, which lead to the dashboard.
#[must_use]
pub fn lockup() -> impl IntoView {
    view! {
        <a href="/" rel="external" class=styles::LOCKUP>
            // The brand mark is the stylesheet's, drawn for the theme; it is
            // decoration, and the product name stands beside it.
            <span class=styles::MARK aria-hidden="true"></span>
            <span class=styles::LOCKUP_NAME>
                "FerroFED " <span class=styles::LOCKUP_ACCENT>"Console"</span>
            </span>
        </a>
    }
}

/// The gateway's status as a pill: `UP` once the gateway answered the
/// console, `DOWN` when the console could not reach it, its status and
/// error code when it refused, and `UNKNOWN` for any other fault.
fn gateway_pill(answer: Result<Outcome<GatewayView>, ViewError>) -> AnyView {
    let (tone, word, detail) = match answer {
        // TODO(#1096): the gateway's product version, once the operator surface reports it.
        Ok(Outcome::Shown(view)) => (
            styles::Tone::Ok,
            String::from("Gateway UP"),
            format!("· spec {} · {}", view.spec_version, view.federation),
        ),
        Ok(Outcome::Refused(Refusal::Gateway { status, code })) => (
            styles::Tone::Danger,
            format!("Gateway refused {status}"),
            code.map(|code| format!("· {code}")).unwrap_or_default(),
        ),
        Ok(Outcome::Refused(Refusal::NotAuthenticated { code })) => (
            styles::Tone::Danger,
            String::from("Gateway refused 401"),
            code.map(|code| format!("· {code}")).unwrap_or_default(),
        ),
        Err(ViewError::Unreachable) => (
            styles::Tone::Danger,
            String::from("Gateway DOWN"),
            String::new(),
        ),
        Ok(Outcome::Refused(Refusal::Invalid { .. })) | Err(_) => (
            styles::Tone::Warn,
            String::from("Gateway UNKNOWN"),
            String::new(),
        ),
    };
    view! { <span class=tone.pill()>{word} <span class="hidden xl:inline">{detail}</span></span> }
        .into_any()
}

/// The theme switch: light, dark and system, the one in force pressed.
///
/// Before the page has hydrated, each button posts its theme to the
/// server function, which keeps it and sends the browser back to the
/// console; once hydrated, a press changes the page at once and keeps the
/// choice through the same function.
fn theme_switch(theme: RwSignal<Theme>) -> AnyView {
    let keep = ServerAction::<SetTheme>::new();
    let buttons = Theme::ALL
        .into_iter()
        .map(|choice| {
            let glyph = match choice {
                Theme::Light => icondata_lu::LuSun,
                Theme::Dark => icondata_lu::LuMoon,
                Theme::System => icondata_lu::LuMonitor,
            };
            let press = move |event: leptos::ev::MouseEvent| {
                event.prevent_default();
                theme.set(choice);
                show(choice);
                keep.dispatch(SetTheme {
                    theme: choice.key().to_owned(),
                    back: None,
                });
            };
            let pressed = move || theme.get() == choice;
            view! {
                <button
                    type="submit"
                    name="theme"
                    value=choice.key()
                    class=styles::SEGMENT
                    aria-pressed=move || pressed().to_string()
                    on:click=press
                >
                    {icon(glyph, "14")}
                    {choice.label()}
                    // The pressed theme is marked by a check and an edge as well
                    // as its ground, never by colour alone (WCAG 2.2 SC 1.4.1).
                    {move || pressed().then(|| icon(icondata_lu::LuCheck, "12"))}
                </button>
            }
        })
        .collect_view();
    let location = leptos_router::hooks::use_location();
    view! {
        <form method="post" action=SetTheme::url() class="m-0 flex flex-col items-end">
            // Before the page has hydrated, the post returns to this page,
            // which the server checks is one of the console's own.
            <input type="hidden" name="back" value=move || location.pathname.get() />
            <div role="group" aria-label="Theme" class=styles::SEGMENTED>
                {buttons}
            </div>
            {move || kept(keep)}
        </form>
    }
    .into_any()
}

/// A notice that the last theme choice was not kept, and why; nothing while
/// none failed (an error never renders as nothing).
fn kept(keep: ServerAction<SetTheme>) -> Option<impl IntoView> {
    let why = keep.value().with(|answer| match answer {
        None | Some(Ok(Outcome::Shown(()))) => None,
        Some(Ok(Outcome::Refused(Refusal::Invalid { reason }))) => Some(reason.clone()),
        Some(Ok(Outcome::Refused(_))) => Some(String::from("the console refused it")),
        Some(Err(error)) => Some(error.to_string()),
    })?;
    Some(view! {
        <p role="status" class="m-0 mt-1 text-xs text-danger-fg">
            "Theme not kept: "
            {why}
        </p>
    })
}

/// Draws the page in `theme` at once, as the server draws the next page:
/// `data-theme` on `<html>`.
fn show(theme: Theme) {
    #[cfg(target_arch = "wasm32")]
    if let Some(root) = document().document_element() {
        // NOTE: no specification governs this: our own design; a browser that
        // refuses the attribute keeps the theme until the next page, which the
        // server draws in the theme the cookie keeps.
        if root.set_attribute("data-theme", theme.key()).is_err() {
            leptos::logging::warn!("the browser refused the theme on the page");
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _: Theme = theme;
}

/// The operator's name, and the menu it opens with the sign-out, and below
/// `sm`, where the top bar has no room for it, the theme switch.
fn operator_menu(operator: String, theme: RwSignal<Theme>) -> impl IntoView {
    let shown = operator.clone();
    view! {
        <button type="button" class=styles::MENU_BUTTON popovertarget=MENU_ID aria-controls=MENU_ID>
            {icon(icondata_lu::LuCircleUser, "16")}
            <span class="max-w-28 truncate sm:max-w-40 md:max-w-24 lg:max-w-40">{operator}</span>
            {icon(icondata_lu::LuChevronDown, "14")}
        </button>
        <div id=MENU_ID popover="auto" class=styles::MENU>
            <p class="m-0 text-sm font-semibold">{shown}</p>
            <p class="mt-0.5 mb-3 text-xs text-muted">"Signed in to this console"</p>
            <div class="mb-3 sm:hidden">{theme_switch(theme)}</div>
            // The sign-out route is the server's: a plain form post it
            // answers with a redirect, which no client router takes.
            <form method="post" action=SIGN_OUT class="m-0">
                <button type="submit" class=format!("{} w-full", styles::BUTTON_SECONDARY)>
                    {icon(icondata_lu::LuLogOut, "16")}
                    "Sign out"
                </button>
            </form>
        </div>
    }
}

/// One entry of the navigation.
#[derive(Clone, Copy)]
struct Entry {
    /// Where it leads.
    href: &'static str,
    /// What it says.
    label: &'static str,
    /// The icon beside it.
    glyph: icondata_core::Icon,
}

/// The navigation entry `entry`, tinted while it is the page: the router's
/// link marks itself `aria-current="page"` there. The dashboard, which the
/// server renders whole, is a full load (`rel="external"`).
fn entry(entry: Entry) -> impl IntoView {
    let exact = entry.href == "/";
    view! {
        <li>
            <A
                href=entry.href
                exact=exact
                attr:rel=exact.then_some("external")
                attr:class=styles::NAV_ENTRY
            >
                {icon(entry.glyph, "16")}
                {entry.label}
            </A>
        </li>
    }
}

/// A group of the navigation under its heading `title`, holding `entries`.
fn group(id: &'static str, title: &'static str, entries: Vec<Entry>) -> impl IntoView {
    let entries = entries.into_iter().map(entry).collect_view();
    view! {
        <p id=id class=styles::NAV_GROUP>
            {title}
        </p>
        <ul class=styles::NAV_LIST aria-labelledby=id>
            {entries}
        </ul>
    }
}

/// The navigation a signed-in operator reads, grouped by what each view is
/// about, with the configuration page when a replica's admin listener is
/// configured.
#[must_use]
pub fn sidebar(administered: bool) -> AnyView {
    let nav = NodeRef::<leptos::html::Nav>::new();
    let overview = vec![Entry {
        href: "/",
        label: "Dashboard",
        glyph: icondata_lu::LuLayoutDashboard,
    }];
    // TODO(#1076): the Routing entry, once the routing table has its own page.
    let federation = vec![
        Entry {
            href: crate::views::MEMBERS,
            label: "Members",
            glyph: icondata_lu::LuUsers,
        },
        Entry {
            href: crate::views::INTEGRITY,
            label: "Integrity",
            glyph: icondata_lu::LuShieldCheck,
        },
    ];
    let queries = vec![
        Entry {
            href: crate::query::QUERY,
            label: "Query console",
            glyph: icondata_lu::LuSquareTerminal,
        },
        Entry {
            href: crate::views::STORED_QUERIES,
            label: "Stored queries",
            glyph: icondata_lu::LuDatabase,
        },
    ];
    let mut gateway = vec![Entry {
        href: crate::views::FEDERATION,
        label: "Self-description",
        glyph: icondata_lu::LuServer,
    }];
    if administered {
        gateway.push(Entry {
            href: crate::configure::CONFIGURATION,
            label: "Configuration",
            glyph: icondata_lu::LuSettings,
        });
    }
    view! {
        <nav
            id=SIDEBAR_ID
            popover="auto"
            aria-label="Views"
            class=styles::SIDEBAR
            node_ref=nav
            on:toggle=move |_| drawer::opened(nav)
            on:click=move |event| drawer::followed(nav, &event)
        >
            <button
                type="button"
                class=format!("{} mb-1 md:hidden", styles::BUTTON_GHOST)
                popovertarget=SIDEBAR_ID
                popovertargetaction="hide"
            >
                {icon(icondata_lu::LuX, "16")}
                "Close"
            </button>
            {group("nav-overview", "Overview", overview)}
            {group("nav-federation", "Federation", federation)}
            {group("nav-queries", "Queries", queries)}
            {group("nav-gateway", "Gateway", gateway)}
        </nav>
    }
    .into_any()
}

/// What the browser half adds to the drawer: the focus moves into it as it
/// opens, and it closes when one of its links is followed (WCAG 2.2 SC
/// 2.4.3). From `md` up the navigation is no popover the browser opens, so
/// neither applies.
mod drawer {
    use leptos::prelude::*;

    /// Moves the focus to the drawer's first link, once it is open.
    pub(super) fn opened(nav: NodeRef<leptos::html::Nav>) {
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast as _;
            let Some(nav) = nav.get_untracked() else {
                return;
            };
            if !nav.matches(":popover-open").unwrap_or(false) {
                return;
            }
            let first = nav
                .query_selector("a")
                .ok()
                .flatten()
                .and_then(|link| link.dyn_into::<leptos::web_sys::HtmlElement>().ok());
            if let Some(first) = first
                && first.focus().is_err()
            {
                leptos::logging::warn!("the browser refused the focus on the navigation");
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _: NodeRef<leptos::html::Nav> = nav;
    }

    /// Closes the open drawer when `event` follows one of its links.
    pub(super) fn followed(nav: NodeRef<leptos::html::Nav>, event: &leptos::ev::MouseEvent) {
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast as _;
            let Some(nav) = nav.get_untracked() else {
                return;
            };
            let on_link = event
                .target()
                .and_then(|target| target.dyn_into::<leptos::web_sys::Element>().ok())
                .and_then(|target| target.closest("a").ok().flatten())
                .is_some();
            if on_link
                && nav.matches(":popover-open").unwrap_or(false)
                && nav.hide_popover().is_err()
            {
                leptos::logging::warn!("the browser refused to close the navigation");
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _: (NodeRef<leptos::html::Nav>, &leptos::ev::MouseEvent) = (nav, event);
    }
}

/// The one line at the foot of every page: the product, its version, its
/// manufacturer, and the About page that names the manufacturer in full
/// (Regulation (EU) 2025/327 Art 30(1)(g)).
#[must_use]
pub fn footer() -> impl IntoView {
    view! {
        <footer class=styles::FOOTER>
            <p class="m-0">
                {PRODUCT} " v" {VERSION} " · " {MANUFACTURER.name} " · "
                <a href=ABOUT rel="external">
                    "About"
                </a>
            </p>
        </footer>
    }
}
