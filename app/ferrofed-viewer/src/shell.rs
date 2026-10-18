// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The chrome around every page, drawn with the `FerroHEALTH` components of
//! `ferrohealth_ui`.
//!
//! It is the top bar with the gateway's status, the operator's menu and the
//! theme toggle; the grouped navigation, a column beside the page from `md`
//! and a drawer below it; and the one-line footer.
//!
//! The drawer and the operator's menu are HTML popovers the top bar's
//! buttons open, so the browser opens them, closes them on Escape and
//! returns the focus to the button, before the page has hydrated and with
//! no script (HTML, the `popover` attribute). The library's navigation adds
//! what HTML leaves out: the focus moves into the drawer as it opens, and
//! the drawer closes when one of its links is followed.

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::theme::Theme;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::menu::{OperatorMenu, operator_menu};
use ferrohealth_ui::view::shell::{About, Footer, brand_mark, drawer_button, footer_about};
use ferrohealth_ui::view::status::{banner, status};
use ferrohealth_ui::widget::nav::{Entry, Group};
use ferrohealth_ui::widget::theme::theme_toggle;
use leptos::prelude::*;
use leptos::server_fn::ServerFn as _;

use crate::app::{ABOUT, PRODUCT, SIGN_OUT, VERSION};
use crate::theme::SetTheme;
use crate::views::model::{DeploymentView, GatewayView, Outcome, Refusal, ViewError};

/// The id of the navigation, which the drawer button opens.
const SIDEBAR_ID: &str = "sidebar";

/// The id of the operator's menu, which the operator's button opens.
const MENU_ID: &str = "operator-menu";

/// The brand mark in the light theme: the gateway's arrow into the
/// federation's three members, in the azure accent.
const MARK_LIGHT: &str = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 64 64'%3E%3Cg stroke='%230369A1' stroke-width='4.4' stroke-linecap='round' fill='none'%3E%3Cpath d='M3 32H16'/%3E%3Cpath d='M11 26.5 16.5 32 11 37.5' stroke-linejoin='round'/%3E%3Cpath d='M27 32 48 14M27 32H50M27 32 48 50'/%3E%3C/g%3E%3Ccircle cx='27' cy='32' r='8' fill='%230369A1'/%3E%3Cg fill='%237DD3FC'%3E%3Crect x='45' y='7' width='14' height='14' rx='3.5'/%3E%3Crect x='47' y='25' width='14' height='14' rx='3.5'/%3E%3Crect x='45' y='43' width='14' height='14' rx='3.5'/%3E%3C/g%3E%3C/svg%3E";

/// The brand mark in the dark theme, its stroke lightened for the dark
/// ground.
const MARK_DARK: &str = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 64 64'%3E%3Cg stroke='%2338BDF8' stroke-width='4.4' stroke-linecap='round' fill='none'%3E%3Cpath d='M3 32H16'/%3E%3Cpath d='M11 26.5 16.5 32 11 37.5' stroke-linejoin='round'/%3E%3Cpath d='M27 32 48 14M27 32H50M27 32 48 50'/%3E%3C/g%3E%3Ccircle cx='27' cy='32' r='8' fill='%2338BDF8'/%3E%3Cg fill='%237DD3FC'%3E%3Crect x='45' y='7' width='14' height='14' rx='3.5'/%3E%3Crect x='47' y='25' width='14' height='14' rx='3.5'/%3E%3Crect x='45' y='43' width='14' height='14' rx='3.5'/%3E%3C/g%3E%3C/svg%3E";

/// What the top bar of a signed-in page shows.
#[derive(Debug, Clone)]
pub struct Bar {
    /// The operator's name, as the provider gave it.
    pub operator: String,
    /// The gateway's status, loaded once for the page.
    pub gateway: Resource<Result<Outcome<GatewayView>, ViewError>>,
    /// How the gateway is deployed, loaded once for the page.
    pub deployment: Resource<Result<Outcome<DeploymentView>, ViewError>>,
}

/// The entry of the dashboard.
const OVERVIEW: [Entry; 1] = [Entry {
    href: "/",
    label: "Dashboard",
    glyph: Glyph::LAYOUT_DASHBOARD,
    exact: true,
}];

/// The entries of the federation's views.
const FEDERATION: [Entry; 3] = [
    Entry {
        href: crate::views::MEMBERS,
        label: "Members",
        glyph: Glyph::USERS,
        exact: false,
    },
    Entry {
        href: crate::views::ROUTING,
        label: "Routing",
        glyph: Glyph::ROUTE,
        exact: false,
    },
    Entry {
        href: crate::views::INTEGRITY,
        label: "Integrity",
        glyph: Glyph::SHIELD_CHECK,
        exact: false,
    },
];

/// The entries of the queries.
const QUERIES: [Entry; 2] = [
    Entry {
        href: crate::query::QUERY,
        label: "Query console",
        glyph: Glyph::SQUARE_TERMINAL,
        exact: false,
    },
    Entry {
        href: crate::views::STORED_QUERIES,
        label: "Stored queries",
        glyph: Glyph::DATABASE,
        exact: false,
    },
];

/// The self-description entry of the gateway's group.
const SELF_DESCRIPTION: Entry = Entry {
    href: crate::views::FEDERATION,
    label: "Self-description",
    glyph: Glyph::SERVER,
    exact: false,
};

/// The gateway's group of a console with no admin listener.
const GATEWAY: [Entry; 1] = [SELF_DESCRIPTION];

/// The gateway's group of a console with a replica's admin listener, which
/// serves the configuration.
const GATEWAY_ADMINISTERED: [Entry; 2] = [
    SELF_DESCRIPTION,
    Entry {
        href: crate::configure::CONFIGURATION,
        label: "Configuration",
        glyph: Glyph::SETTINGS,
        exact: false,
    },
];

/// The navigation, grouped by what each view is about, with `gateway` as
/// the gateway's group.
const fn navigation(gateway: &'static [Entry]) -> [Group; 4] {
    [
        Group {
            id: "nav-overview",
            title: "Overview",
            entries: &OVERVIEW,
        },
        Group {
            id: "nav-federation",
            title: "Federation",
            entries: &FEDERATION,
        },
        Group {
            id: "nav-queries",
            title: "Queries",
            entries: &QUERIES,
        },
        Group {
            id: "nav-gateway",
            title: "Gateway",
            entries: gateway,
        },
    ]
}

/// The navigation of a console with no admin listener.
const NAVIGATION: [Group; 4] = navigation(&GATEWAY);

/// The navigation of a console with a replica's admin listener.
const NAVIGATION_ADMINISTERED: [Group; 4] = navigation(&GATEWAY_ADMINISTERED);

/// The mark and the product name, which lead to the dashboard.
#[must_use]
pub fn lockup() -> impl IntoView {
    ferrohealth_ui::view::shell::lockup(
        "/",
        brand_mark(MARK_LIGHT, MARK_DARK),
        "FerroFED",
        "Console",
    )
}

/// The top bar of a signed-in page: the drawer button, the lockup, and at
/// the far end the gateway's status, the operator and the theme toggle.
#[must_use]
pub fn topbar(bar: Bar) -> AnyView {
    let (gateway, deployment) = (bar.gateway, bar.deployment);
    let state = view! {
        <Suspense fallback=|| {
            status(Tone::Neutral, Glyph::SERVER, "Gateway", None)
        }>
            {move || Suspend::new(async move { gateway_status(&gateway.await, &deployment.await) })}
        </Suspense>
    };
    let location = leptos_router::hooks::use_location();
    // NOTE: no specification governs this: our own design; the toggle swaps the
    // theme in place and the server function keeps it in the session's cookie.
    let keep = ServerAction::<SetTheme>::new();
    let chosen = Callback::new(move |theme: Theme| {
        keep.dispatch(SetTheme {
            theme: theme.key().to_owned(),
            back: None,
        });
    });
    let switch = move || theme_toggle(SetTheme::url().to_owned(), location.pathname.get(), chosen);
    let start = view! {
        {drawer_button(SIDEBAR_ID)}
        {lockup()}
    };
    let end = view! {
        <span class="fui-roomy-only">{state}</span>
        {operator_menu(
            OperatorMenu {
                id: MENU_ID,
                name: bar.operator,
                subtitle: "Signed in to this console",
                sign_out: Some(String::from(SIGN_OUT)),
            },
            (),
        )}
        {switch}
        {move || kept(keep)}
    };
    ferrohealth_ui::view::shell::topbar(start, end).into_any()
}

/// Returns the tone, the words and the detail of the gateway's status.
///
/// They are `UP` once the gateway answered the console, with its version
/// and replica, `DOWN` when the console could not reach it, its status and
/// error code when it refused, and `UNKNOWN` for any other fault.
#[must_use]
pub fn gateway_words(
    answer: &Result<Outcome<GatewayView>, ViewError>,
    deployment: &Result<Outcome<DeploymentView>, ViewError>,
) -> (Tone, String, Option<String>) {
    match answer {
        Ok(Outcome::Shown(view)) => (
            Tone::Ok,
            String::from("Gateway UP"),
            Some(match deployment {
                Ok(Outcome::Shown(deployed)) => match &deployed.replica {
                    Some(replica) => format!("· v{} · {replica}", deployed.version),
                    None => format!("· v{}", deployed.version),
                },
                // NOTE: no specification governs this: our own design; an operator the
                // deployment is refused to reads the status without the version.
                _ => format!("· spec {} · {}", view.spec_version, view.federation),
            }),
        ),
        Ok(Outcome::Refused(Refusal::Gateway { status, code })) => (
            Tone::Bad,
            format!("Gateway refused {status}"),
            code.as_ref().map(|code| format!("· {code}")),
        ),
        Ok(Outcome::Refused(Refusal::NotAuthenticated { code })) => (
            Tone::Bad,
            String::from("Gateway refused 401"),
            code.as_ref().map(|code| format!("· {code}")),
        ),
        Err(ViewError::Unreachable) => (Tone::Bad, String::from("Gateway DOWN"), None),
        Ok(Outcome::Refused(Refusal::Invalid { .. })) | Err(_) => {
            (Tone::Warn, String::from("Gateway UNKNOWN"), None)
        }
    }
}

/// The gateway's status: an icon and a word, never a colour alone, with
/// its version or its refusal's code after it.
fn gateway_status(
    answer: &Result<Outcome<GatewayView>, ViewError>,
    deployment: &Result<Outcome<DeploymentView>, ViewError>,
) -> AnyView {
    let (tone, words, detail) = gateway_words(answer, deployment);
    let glyph = match tone {
        Tone::Ok => Glyph::CIRCLE_CHECK,
        Tone::Bad => Glyph::SERVER_CRASH,
        _ => Glyph::TRIANGLE_ALERT,
    };
    status(tone, glyph, &words, detail).into_any()
}

/// The amber banner under the top bar of a development gateway.
///
/// It names the development cross-reference when it is in service, and
/// shows nothing for any other deployment, or when the gateway does not say.
#[must_use]
pub fn deployment_banner(
    deployment: Resource<Result<Outcome<DeploymentView>, ViewError>>,
) -> AnyView {
    view! {
        <Suspense fallback=|| ()>
            {move || Suspend::new(async move {
                match deployment.await {
                    Ok(Outcome::Shown(deployed)) if deployed.development => {
                        Some(development(deployed.development_crossref))
                    }
                    _ => None,
                }
            })}
        </Suspense>
    }
    .into_any()
}

/// The words of the development banner, with the cross-reference when it
/// is in service.
fn development(crossref: bool) -> impl IntoView {
    let mut body = String::from(
        "this gateway runs the development profile and must not hold real patient data.",
    );
    if crossref {
        body.push_str(" Patients resolve through the static development cross-reference.");
    }
    banner(
        Tone::Warn,
        Glyph::TRIANGLE_ALERT,
        "Development deployment",
        &body,
    )
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
        <span role="status">
            {status(Tone::Bad, Glyph::CIRCLE_ALERT, "Theme not kept", Some(format!("· {why}")))}
        </span>
    })
}

/// The navigation a signed-in operator reads, grouped by what each view is
/// about, with the configuration page when a replica's admin listener is
/// configured.
#[must_use]
pub fn sidebar(administered: bool) -> AnyView {
    let groups = if administered {
        &NAVIGATION_ADMINISTERED
    } else {
        &NAVIGATION
    };
    ferrohealth_ui::widget::nav::sidebar(SIDEBAR_ID, "Views", groups)
}

/// The one line at the foot of every page: the product, its version, its
/// manufacturer, and the About page that names the manufacturer in full
/// (Regulation (EU) 2025/327 Art 30(1)(g)).
///
/// The About page is static markup the server renders, so its link is a
/// full load the client router never takes.
#[must_use]
pub fn footer() -> impl IntoView {
    footer_about(footing(), About::FullLoad)
}

/// What the footer names: the product, its version and the About page.
#[must_use]
pub const fn footing() -> Footer {
    Footer {
        product: PRODUCT,
        version: VERSION,
        about: ABOUT,
    }
}
