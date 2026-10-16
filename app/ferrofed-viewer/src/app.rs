// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The page tree the server renders and the browser hydrates.
//!
//! It holds the document in the operator's theme, the shell a signed-in
//! operator reads, the centred pages a visitor reads, the routes, and the
//! pages for a path the console does not serve and for a session store it
//! cannot read.

use leptos::either::{Either, EitherOf3};
use leptos::prelude::*;
use leptos_meta::{Stylesheet, Title, provide_meta_context};
use leptos_router::components::{Route, Router, Routes};
use leptos_router::{SsrMode, path};

use crate::components::{empty_state, icon};
use crate::theme::Theme;
use crate::{about, home, shell, styles, views};

/// The product name as an operator reads it.
pub const PRODUCT: &str = "FerroFED Console";

/// The product version the footer names.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The path of the sign-in route the server serves.
pub const SIGN_IN: &str = "/login";

/// The path of the sign-out route the server serves.
pub const SIGN_OUT: &str = "/logout";

/// The path of the About page, which every page's footer links to.
pub const ABOUT: &str = "/about";

/// The whole HTML document the server renders around [`App`], in the theme
/// the request's cookie names, so the page arrives in it before any script
/// runs.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn shell(options: LeptosOptions) -> impl IntoView {
    let theme = crate::theme::server::requested().key();
    view! {
        <!DOCTYPE html>
        <html lang="en" data-theme=theme>
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <AutoReload options=options.clone() />
                <HydrationScripts options />
                <leptos_meta::MetaTags />
            </head>
            <body>
                <App />
            </body>
        </html>
    }
}

/// The root component: the stylesheet, the document title, and the shell
/// the request's session is drawn in.
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    // NOTE: the Leptos book, `ssr/22_life_cycle`: a value the server decides reaches
    // the hydrating page serialized, so both halves draw the same shell.
    let administered = *SharedValue::new_str(administered);
    let session = *SharedValue::new_str(session_state);
    let theme = *SharedValue::new_str(theme_requested);
    let gateway = SharedValue::new_str(gateway_base).into_inner();
    let bar = (session == SessionState::SignedIn).then(|| shell::Bar {
        operator: SharedValue::new_str(operator_name).into_inner(),
        theme: RwSignal::new(theme),
        gateway: Resource::new_blocking(|| (), |()| views::load::gateway_status()),
    });
    view! {
        <Stylesheet id="leptos" href="/pkg/ferrofed-viewer.css" />
        <Title text=PRODUCT />
        <Router>{layout(session, bar, administered, gateway)}</Router>
    }
}

/// The page for a request whose session is `session`: the shell of a
/// signed-in operator with its `bar`, or the page alone for a visitor and
/// for a session store the console cannot read.
///
/// It is built inside the router, whose context the routes and the
/// navigation's links read.
fn layout(
    session: SessionState,
    bar: Option<shell::Bar>,
    administered: bool,
    gateway: String,
) -> AnyView {
    // NOTE: leptos_axum, `generate_route_list`: the server lists its routes by
    // rendering the app with no request, so every shell draws the same routes.
    match bar {
        Some(bar) => signed_in(bar, administered, routes(session, gateway)).into_any(),
        None => visitor(routes(session, gateway)).into_any(),
    }
}

/// The shell of a signed-in operator: the top bar, the navigation beside
/// the page, and the footer.
fn signed_in(bar: shell::Bar, administered: bool, routes: AnyView) -> impl IntoView {
    view! {
        <div class=styles::PAGE>
            <a
                href="#main"
                class="sr-only rounded-control bg-raised px-3 py-2 focus:not-sr-only focus:absolute focus:top-2 focus:left-2 focus:z-50"
            >
                "Skip to the page"
            </a>
            {shell::topbar(bar)}
            <div class=styles::BODY>
                {shell::sidebar(administered)} <main id="main" class=styles::MAIN>
                    <div class=styles::COLUMN>{routes}</div>
                </main>
            </div>
            {shell::footer()}
        </div>
    }
}

/// The shell of a visitor: the page alone, and the footer.
fn visitor(page: AnyView) -> impl IntoView {
    view! {
        <div class=styles::PAGE>
            <main id="main" class="flex flex-1 flex-col px-4 py-8">
                {page}
            </main>
            {shell::footer()}
        </div>
    }
}

/// The routes, for a request whose session is `session`; `gateway` is the
/// gateway's `{base}` the About page shows a signed-in operator.
fn routes(session: SessionState, gateway: String) -> AnyView {
    // NOTE: the Leptos book, `ssr/23_ssr_modes`: a page the gateway fills
    // renders `Async`, whole in one sweep, so no content waits on a script.
    view! {
        <Routes fallback=move || {
            if session == SessionState::Unreadable {
                Either::Left(unreadable())
            } else {
                Either::Right(view! { <NotFound /> })
            }
        }>
            <Route
                path=path!("/")
                view=move || match session {
                    SessionState::SignedIn => EitherOf3::A(view! { <home::DashboardPage /> }),
                    SessionState::SignedOut => EitherOf3::B(view! { <home::SignInPage /> }),
                    SessionState::Unreadable => EitherOf3::C(unreadable()),
                }
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/about")
                view=move || view! { <about::AboutPage gateway=gateway.clone() /> }
                ssr=SsrMode::Async
            />
            <Route path=path!("/members") view=views::MembersPage ssr=SsrMode::Async />
            <Route path=path!("/integrity") view=views::IntegrityPage ssr=SsrMode::Async />
            <Route path=path!("/stored-queries") view=views::StoredQueriesPage ssr=SsrMode::Async />
            <Route path=path!("/federation") view=views::FederationPage ssr=SsrMode::Async />
            <Route path=path!("/query") view=crate::query::QueryPage ssr=SsrMode::Async />
            <Route
                path=path!("/configuration")
                view=crate::configure::ConfigurationPage
                ssr=SsrMode::Async
            />
        </Routes>
    }
    .into_any()
}

/// The page a visitor reads when the console cannot read its session store:
/// it cannot tell whether they are signed in, so it offers neither the
/// sign-in nor the views, and answers `500`.
fn unreadable() -> impl IntoView {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(response) = use_context::<leptos_axum::ResponseOptions>() {
        response.set_status(http::StatusCode::INTERNAL_SERVER_ERROR);
    }
    let again = view! {
        <a href="/" rel="external" class=styles::BUTTON_SECONDARY>
            {icon(icondata_lu::LuRotateCw, "16")}
            "Try again"
        </a>
    };
    let body = view! {
        <p role="alert" class=styles::EMPTY_BODY>
            "500: the console cannot read its session store, so it cannot tell whether you "
            "are signed in. Ask the console's operator to restart it."
        </p>
    };
    view! {
        <Title text=views::titled("Fault") />
        <div class=styles::CENTRED>
            {empty_state(
                icondata_lu::LuServerCrash,
                "The console cannot read its sessions",
                body,
                again,
            )}
        </div>
    }
}

/// The page for a path the console does not serve: `404`, and the way back.
#[component]
fn NotFound() -> impl IntoView {
    // NOTE: RFC 9110 §15.6: an unreadable session store is the server's own
    // fault, so the 5xx the shell set outranks the 404 of an unknown path.
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(response) = use_context::<leptos_axum::ResponseOptions>() {
        let failed = response
            .0
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .status
            .is_some_and(|status| status.is_server_error());
        if !failed {
            response.set_status(http::StatusCode::NOT_FOUND);
        }
    }
    let back = view! {
        <a href="/" rel="external" class=styles::BUTTON_PRIMARY>
            {icon(icondata_lu::LuArrowLeft, "16")}
            "Back to the console"
        </a>
    };
    let body =
        view! { <p class=styles::EMPTY_BODY>"404: the console has no page at this address."</p> };
    view! {
        <Title text=views::titled("Not found") />
        <div class=styles::CENTRED>
            {empty_state(icondata_lu::LuCompass, "Not found", body, back)}
        </div>
    }
}

/// Whether the console is configured with a replica's admin listener, which
/// the configuration page and the admin actions need.
fn administered() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use_context::<crate::server::ViewerState>()
            .is_some_and(|state| !state.settings().admin.is_empty())
    }
    // NOTE: the browser reads the value the server decided; this runs only if
    // that value is missing, and then shows the link the page explains.
    #[cfg(target_arch = "wasm32")]
    {
        true
    }
}

/// The theme the request's cookie names.
fn theme_requested() -> Theme {
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::theme::server::requested()
    }
    // NOTE: the browser reads the value the server decided; this runs only if
    // that value is missing, and then marks the theme that follows the system.
    #[cfg(target_arch = "wasm32")]
    {
        Theme::default()
    }
}

/// The name the top bar shows for a signed-in operator: the provider's name
/// for them, or `Operator` when it gave none.
fn operator_name() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        views::load::server::operator().unwrap_or_else(|| String::from(OPERATOR))
    }
    // NOTE: the browser reads the value the server decided; this runs only if
    // that value is missing, and then shows the word a nameless sign-in shows.
    #[cfg(target_arch = "wasm32")]
    {
        String::from(OPERATOR)
    }
}

/// The word the top bar shows for an operator the provider gave no name.
const OPERATOR: &str = "Operator";

/// The gateway's `{base}`, which the About page shows a signed-in operator,
/// and empty for a visitor.
fn gateway_base() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let signed_in = session_state() == SessionState::SignedIn;
        use_context::<crate::server::ViewerState>()
            .filter(|_state| signed_in)
            .map(|state| state.settings().gateway.base.to_string())
            .unwrap_or_default()
    }
    // NOTE: the browser reads the value the server decided; this runs only if
    // that value is missing, and then shows no gateway.
    #[cfg(target_arch = "wasm32")]
    {
        String::new()
    }
}

/// What the server knows of the request's session, which decides the shell
/// the page is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// A live signed-in session: the navigation and the sign-out.
    SignedIn,
    /// No live session: the sign-in, and neither the navigation nor the
    /// sign-out.
    SignedOut,
    /// The session store cannot be read, so the console cannot tell: the
    /// page says so, and offers neither the sign-in nor the views.
    Unreadable,
}

impl SessionState {
    /// The state a session lookup's answer `looked_up` stands for: only a
    /// missing or ended session is signed out, and any other fault is
    /// unreadable.
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn of<T>(looked_up: &Result<T, views::model::ViewError>) -> Self {
        match looked_up {
            Ok(_) => Self::SignedIn,
            Err(views::model::ViewError::SignedOut) => Self::SignedOut,
            Err(_) => Self::Unreadable,
        }
    }
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::SignedIn => "in",
            Self::SignedOut => "out",
            Self::Unreadable => "unreadable",
        })
    }
}

impl std::str::FromStr for SessionState {
    type Err = UnknownSessionState;

    fn from_str(said: &str) -> Result<Self, Self::Err> {
        match said {
            "in" => Ok(Self::SignedIn),
            "out" => Ok(Self::SignedOut),
            "unreadable" => Ok(Self::Unreadable),
            _ => Err(UnknownSessionState),
        }
    }
}

/// A serialized [`SessionState`] the browser cannot read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the page names a session state the console does not know")]
pub struct UnknownSessionState;

/// The state of the request's session, which the shell is drawn for.
fn session_state() -> SessionState {
    #[cfg(not(target_arch = "wasm32"))]
    {
        SessionState::of(&views::load::server::signed_in())
    }
    // NOTE: the browser reads the value the server decided; this runs only if
    // that value is missing, and then draws the page a visitor sees.
    #[cfg(target_arch = "wasm32")]
    {
        SessionState::SignedOut
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::SessionState;
    use crate::views::model::ViewError;

    #[test]
    fn only_a_missing_session_reads_as_signed_out_and_a_broken_store_as_unreadable() {
        assert_eq!(
            SessionState::SignedIn,
            SessionState::of(&Ok::<(), ViewError>(()))
        );
        assert_eq!(
            SessionState::SignedOut,
            SessionState::of(&Err::<(), _>(ViewError::SignedOut))
        );
        assert_eq!(
            SessionState::Unreadable,
            SessionState::of(&Err::<(), _>(ViewError::Unavailable))
        );
    }

    #[test]
    fn the_state_the_page_carries_reads_back_as_itself() {
        for state in [
            SessionState::SignedIn,
            SessionState::SignedOut,
            SessionState::Unreadable,
        ] {
            assert_eq!(Ok(state), state.to_string().parse());
        }
        assert!("signed in".parse::<SessionState>().is_err());
    }
}
