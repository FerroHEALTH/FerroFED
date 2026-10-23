// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The page tree the server renders and the browser hydrates.
//!
//! It holds the document in the operator's theme and the request's locale
//! (`crate::locale`), the shell a signed-in
//! operator reads, the centred pages a visitor reads, the routes, and the
//! pages for a path the console does not serve and for a session store it
//! cannot read.

use ferrohealth_ui::i18n::{Locale, provide_locale};
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::button::{Variant, label};
use ferrohealth_ui::view::empty::empty_state;
use ferrohealth_ui::view::refusal::fault;
use ferrohealth_ui::view::shell::{About, MAIN_ID, Shell, shell_about};
use leptos::either::{Either, EitherOf3};
use leptos::prelude::*;
use leptos_meta::{Stylesheet, Title, provide_meta_context};
use leptos_router::components::{Route, Router, Routes};
use leptos_router::{SsrMode, path};

use crate::locale::{Msg, tr};
use crate::{about, home, shell, views};

/// The product name as an operator reads it.
pub const PRODUCT: &str = "FerroFED Console";

/// The path the server serves the `FerroHEALTH` stylesheet at, beside the
/// site bundle.
pub const STYLESHEET: &str = "/pkg/ferrohealth-ui.css";

/// The path the server serves the console's brand theme at, which loads
/// after the stylesheet.
pub const THEME: &str = "/pkg/ferrofed-theme.css";

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
    let lang = crate::locale::requested().tag();
    view! {
        <!DOCTYPE html>
        <html lang=lang data-theme=theme>
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
    let tag = SharedValue::new_str(locale_requested).into_inner();
    provide_locale(Locale::parse(&tag).unwrap_or_default());
    let session = *SharedValue::new_str(session_state);
    let gateway = SharedValue::new_str(gateway_base).into_inner();
    let bar = (session == SessionState::SignedIn).then(|| shell::Bar {
        operator: SharedValue::new_str(operator_name).into_inner(),
        gateway: Resource::new_blocking(|| (), |()| views::load::gateway_status()),
        deployment: Resource::new_blocking(|| (), |()| views::load::deployment()),
    });
    view! {
        <Stylesheet id="ferrohealth-ui" href=STYLESHEET />
        <Stylesheet id="theme" href=THEME />
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
fn signed_in(bar: shell::Bar, administered: bool, routes: AnyView) -> AnyView {
    let deployment = bar.deployment;
    shell_about(
        Shell {
            topbar: shell::topbar(bar),
            banner: Some(shell::deployment_banner(deployment)),
            sidebar: shell::sidebar(administered),
            content: routes,
            footer: shell::footing(),
        },
        About::FullLoad,
    )
}

/// The shell of a visitor: the page alone, and the footer.
fn visitor(page: AnyView) -> AnyView {
    let foot = shell::footer().into_any();
    view! {
        <div class="fui-page">
            <main id=MAIN_ID class="fui-main">
                <div class="fui-column">{page}</div>
            </main>
            {foot}
        </div>
    }
    .into_any()
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
            <Route
                path=path!("/members")
                view=guarded(session, views::members::MembersPage)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/members/:node")
                view=guarded(session, views::members::MemberPage)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/integrity")
                view=guarded(session, views::integrity::IntegrityPage)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/routing")
                view=guarded(session, views::routing::RoutingPage)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/stored-queries")
                view=guarded(session, views::stored::StoredQueriesPage)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/stored-queries/:name")
                view=guarded(session, views::stored::StoredQueryPage)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/federation")
                view=guarded(session, views::describe::FederationPage)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/query")
                view=guarded(session, crate::query::QueryPage)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/configuration")
                view=guarded(session, crate::configure::ConfigurationPage)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/configuration/:tab")
                view=guarded(session, crate::configure::ConfigurationPage)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/configuration/:tab/:item")
                view=guarded(session, crate::configure::ConfigurationPage)
                ssr=SsrMode::Async
            />
        </Routes>
    }
    .into_any()
}

/// The view `page` for a request whose session is `session`, or the
/// unreadable store's page when the console cannot read its session store,
/// so a view never answers that fault with its own generic notice.
fn guarded<V>(session: SessionState, page: fn() -> V) -> impl Fn() -> AnyView + Clone + Send + Sync
where
    V: IntoView + 'static,
{
    move || {
        if session == SessionState::Unreadable {
            unreadable().into_any()
        } else {
            page().into_any()
        }
    }
}

/// The page a visitor reads when the console cannot read its session store:
/// it cannot tell whether they are signed in, so it offers neither the
/// sign-in nor the views, and answers `500`.
fn unreadable() -> impl IntoView {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(response) = use_context::<leptos_axum::ResponseOptions>() {
        response.set_status(http::StatusCode::INTERNAL_SERVER_ERROR);
    }
    let alert = fault(tr(Msg::UnreadableFault));
    view! {
        <Title text=views::titled(&tr(Msg::UnreadableTitle)) />
        <div class="fui-centred">
            <div class="fui-card fui-sign-in">
                <h1>{tr(Msg::UnreadableHeading)}</h1>
                {alert}
                <a href="/" rel="external" class=Variant::Secondary.class()>
                    {label(Some(Glyph::REFRESH_CW), tr(Msg::UnreadableRetry))}
                </a>
            </div>
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
        <a href="/" rel="external" class=Variant::Primary.class()>
            {label(Some(Glyph::ARROW_LEFT), tr(Msg::NotFoundBack))}
        </a>
    };
    let empty = empty_state(
        Glyph::COMPASS,
        tr(Msg::NotFoundTitle),
        tr(Msg::NotFoundBody),
        back,
    );
    view! {
        <Title text=views::titled(&tr(Msg::NotFoundTitle)) />
        <div class="fui-centred">
            <h1 class="fui-sr-only">{tr(Msg::NotFoundTitle)}</h1>
            {empty}
        </div>
    }
}

/// The tag of the locale the request is served in.
fn locale_requested() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::locale::requested().tag()
    }
    // NOTE: the browser reads the value the server decided; this runs only if
    // that value is missing, and then draws the reference locale.
    #[cfg(target_arch = "wasm32")]
    {
        Locale::english().tag()
    }
}

/// Whether the console is configured with a replica's admin listener, which
/// the configuration page and the admin actions need.
fn administered() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use_context::<crate::server::ConsoleState>()
            .is_some_and(|state| !state.settings().admin.is_empty())
    }
    // NOTE: the browser reads the value the server decided; this runs only if
    // that value is missing, and then shows the link the page explains.
    #[cfg(target_arch = "wasm32")]
    {
        true
    }
}

/// The name the top bar shows for a signed-in operator: the provider's name
/// for them, or `Operator` when it gave none.
fn operator_name() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        views::load::server::operator().unwrap_or_else(unnamed_operator)
    }
    // NOTE: the browser reads the value the server decided; this runs only if
    // that value is missing, and then shows the word a nameless sign-in shows.
    #[cfg(target_arch = "wasm32")]
    {
        unnamed_operator()
    }
}

/// The word the top bar shows for an operator the provider gave no name.
fn unnamed_operator() -> String {
    tr(Msg::Operator)
}

/// The gateway's `{base}`, which the About page shows a signed-in operator,
/// and empty for a visitor.
fn gateway_base() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let signed_in = session_state() == SessionState::SignedIn;
        use_context::<crate::server::ConsoleState>()
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
