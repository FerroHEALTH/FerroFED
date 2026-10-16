// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stored queries the gateway holds: the list grouped by namespace, and
//! the detail of one query with its versions and its AQL.
//!
//! Both are read from the gateway's operator surface as the signed-in
//! operator and rendered on the server ([`render`]), so the browser shows
//! the HTML the server wrote and carries none of its drawing. The list names
//! each version and never its text; the detail shows the text of one
//! version, highlighted. A stored query's name is the definition's
//! qualified name (ITS-REST Query API, "Qualified query name"), never a
//! patient identifier, so it can sit in the detail's path (§5.4.1, N33).

#[cfg(not(target_arch = "wasm32"))]
pub mod render;

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::{use_params_map, use_query_map};
use serde::{Deserialize, Serialize};

use crate::admin::act::RepairDrift;
use crate::components::page_header;
use crate::views::model::{Outcome, ViewError};
use crate::views::{STORED_QUERIES, offset_in_url, pager, replica_line, shown, titled};

/// The query parameter of the query console that names the stored query to
/// run, by its qualified name.
pub const CONSOLE_NAME: &str = "stored";

/// The query parameter of the query console that names the version of the
/// stored query to run.
pub const CONSOLE_VERSION: &str = "version";

/// The query parameter of the detail that names the version it shows.
pub const VERSION: &str = "version";

/// One page of the stored-query list, as the server rendered it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredView {
    /// The namespace cards of the page, rendered on the server.
    pub html: String,
    /// Where the page starts in the registry.
    pub offset: u64,
    /// How many versions the page names.
    pub shown: u32,
    /// How many versions the registry holds.
    pub total: u64,
    /// The drift repair's fields, rendered on the server; `None` when the
    /// console is configured with no admin listener.
    pub repair_form: Option<String>,
    /// Which replica answered the page, as the view says it; `None` when
    /// the replica has no name.
    pub answered_by: Option<String>,
}

/// The detail of one stored query, as the server rendered it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredDetail {
    /// The detail, rendered on the server.
    pub html: String,
}

/// Loads the page of the stored-query list that starts at `offset`.
///
/// # Errors
/// Returns [`ViewError::SignedOut`] without a live session, and the
/// console's fault as its [`ViewError`]; the gateway's refusal is an
/// [`Outcome::Refused`].
#[server(endpoint = "stored-queries")]
pub async fn stored_queries(
    /// Where the page of held versions starts.
    offset: u64,
) -> Result<Outcome<StoredView>, ViewError> {
    use crate::views::load::server;
    server::settled(async {
        let (state, token) = server::signed_in()?;
        let report = state
            .gateway()
            .stored_queries(&token, server::page(offset))
            .await?;
        Ok(render::list(&report, &state.settings().admin))
    })
    .await
}

/// Loads the detail of the stored query `name`, showing its version
/// `version`, or its newest when `version` is `None`.
///
/// # Errors
/// As [`stored_queries`].
#[server(endpoint = "stored-query")]
pub async fn stored_query(
    /// The qualified name of the stored query.
    name: String,
    /// The version to show.
    version: Option<String>,
) -> Result<Outcome<StoredDetail>, ViewError> {
    use crate::views::load::server;
    server::settled(async {
        let (state, token) = server::signed_in()?;
        let (held, replica) = render::versions_of(state.gateway(), &token, &name).await?;
        Ok(render::detail(
            &name,
            &held,
            version.as_deref(),
            replica.as_deref(),
        ))
    })
    .await
}

/// The stored queries the gateway holds, grouped by namespace.
#[component]
pub fn StoredQueriesPage() -> impl IntoView {
    let loaded = Resource::new_blocking(offset_in_url(), stored_queries);
    let repair = ServerAction::<RepairDrift>::new();
    view! {
        <Title text=titled("Stored queries") />
        {page_header("Stored queries", "The queries the gateway holds, by namespace.", ())}
        <Transition fallback=|| {
            view! { <p>"Loading the stored queries."</p> }
        }>
            {move || Suspend::new(async move {
                shown(loaded.await, move |view| list_section(view, repair))
            })}
        </Transition>
    }
}

/// The namespace cards of a page, its pager and the drift repair.
fn list_section(view: StoredView, repair: ServerAction<RepairDrift>) -> AnyView {
    let answered_by = replica_line(view.answered_by);
    let stored_pager = (view.total > 0).then(|| {
        pager(
            STORED_QUERIES,
            view.offset,
            usize::try_from(view.shown).unwrap_or(usize::MAX),
            view.total,
        )
    });
    let offered = view.repair_form.is_some();
    let repair = crate::admin::repair_section(repair, view.repair_form);
    let repair = offered.then(|| {
        view! { <div class=format!("{} mt-6", crate::styles::CARD_PAD)>{repair}</div> }
    });
    view! {
        {answered_by}
        <div class="contents" inner_html=view.html></div>
        {stored_pager}
        {repair}
    }
    .into_any()
}

/// One stored query: its versions, the one shown, and its AQL.
#[component]
#[expect(
    clippy::must_use_candidate,
    reason = "the component macro writes the function it returns without the attributes on the one written here (https://docs.rs/leptos/0.8/leptos/attr.component.html)"
)]
pub fn StoredQueryPage() -> impl IntoView {
    let params = use_params_map();
    let query = use_query_map();
    let name = move || params.read().get("name").unwrap_or_default();
    let asked = move || (name(), query.read().get(VERSION));
    let loaded = Resource::new_blocking(asked, |(name, version)| stored_query(name, version));
    view! {
        <Title text=move || titled(&name()) />
        <Transition fallback=|| {
            view! { <p>"Loading the stored query."</p> }
        }>
            {move || Suspend::new(async move {
                shown(
                    loaded.await,
                    |detail| {
                        view! { <div class="contents" inner_html=detail.html></div> }.into_any()
                    },
                )
            })}
        </Transition>
    }
}

/// The path of the detail of version `version` of the stored query `name`.
#[must_use]
pub fn detail_href(name: &str, version: &str) -> String {
    format!(
        "{STORED_QUERIES}/{}?{VERSION}={}",
        encoded(name),
        encoded(version)
    )
}

/// The path of the query console with version `version` of the stored
/// query `name` chosen to run: the name and the version, never its AQL or
/// a parameter.
#[must_use]
pub fn console_href(name: &str, version: &str) -> String {
    format!(
        "{}?{CONSOLE_NAME}={}&{CONSOLE_VERSION}={}",
        crate::query::QUERY,
        encoded(name),
        encoded(version)
    )
}

/// `text` percent-encoded for a path segment or a query value: every byte
/// but an ASCII letter, digit, `-`, `.`, `_`, `~` or `:` (RFC 3986 §2.1,
/// §3.3).
fn encoded(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~:".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push('%');
            // NOTE: no specification governs this: our own design; a nibble is
            // always a hexadecimal digit, so the filter keeps both.
            out.extend(
                [byte >> 4, byte & 0x0F]
                    .into_iter()
                    .filter_map(|nibble| char::from_digit(u32::from(nibble), 16))
                    .map(|digit| digit.to_ascii_uppercase()),
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{console_href, detail_href};

    #[test]
    fn a_qualified_name_reads_as_itself_in_the_path_and_anything_else_is_encoded() {
        assert_eq!(
            "/stored-queries/org.example::compositions?version=1.0.0",
            detail_href("org.example::compositions", "1.0.0")
        );
        assert_eq!(
            "/stored-queries/org.example::a%2Fb%20c%3F%C3%BC?version=1.0.0%26x",
            detail_href("org.example::a/b c?ü", "1.0.0&x")
        );
    }

    #[test]
    fn the_console_link_names_the_query_and_its_version_alone() {
        assert_eq!(
            "/query?stored=org.example::compositions&version=1.0.2",
            console_href("org.example::compositions", "1.0.2")
        );
    }
}
