// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The server's rendering of the configuration page and of each form's
//! answer, which the page shows as it stands.
//!
//! The page is a header with the revision that runs and the one saved, the
//! card of what that saved revision changes, and five tabs, each at a path
//! of its own: the members, the credentials and binding sections, the
//! stored queries, the history and the import and export. A member's, a
//! credential's, a binding section's and a stored query's form is a page
//! of its own under its tab.
//!
//! Leptos escapes every text and attribute value it renders, so a member's
//! name, a revision's reason, a diff's value or a stored query's name
//! arrives as text, never as markup. No data is placed as a child of a
//! `textarea`: a field the page fills in carries its value as an attribute.

pub mod answers;
mod bindings;
mod credentials;
mod exchange;
mod members;
mod parts;
mod queries;
mod top;

use ferrofed_registry::console::Resources;
use ferrofed_registry::operator::Page;
use ferrofed_registry::revision::{
    Author, Current, Difference, Outcome, Revision, RevisionId, Standing,
};
use ferrohealth_ui::view::time::reading;
use leptos::prelude::*;

use crate::config::settings::ReplicaSettings;
use crate::configure::model::{ConfigView, Screen};
use crate::views::model::Refusal;

/// What a replica reports of a saved revision, with the preview of what it
/// changes against the running revision when the console could read it.
#[derive(Debug, Clone)]
pub struct SavedLine {
    /// The revision saved.
    pub revision: Revision,
    /// The revision that runs, which the preview compares with.
    pub running: Option<RevisionId>,
    /// The keys that differ, `None` when the preview could not be read.
    pub changes: Option<Vec<Difference>>,
}

/// What a replica reports of a stored-query version stored.
#[derive(Debug, Clone)]
pub struct StoredLine {
    /// The qualified name.
    pub name: String,
    /// The version.
    pub version: String,
}

/// The id on the page of the reason field of the rollback.
pub const ROLLBACK_REASON: &str = "cfg-rollback-reason";

/// What the page is drawn from, as the first replica answered it.
#[derive(Debug, Clone, Copy)]
pub struct Loaded<'a> {
    /// The replica the page reads and changes.
    pub replica: &'a ReplicaSettings,
    /// Which revision runs and which is newest.
    pub standing: &'a Standing,
    /// Every member, stored query, credential and binding section.
    pub resources: &'a Resources,
    /// The newest revisions, for the history.
    pub history: Option<&'a Page<Revision>>,
    /// What the newest revision changes against the running one, when it
    /// does not run.
    pub pending: Option<&'a [Difference]>,
    /// The revision the history compares with the running one, and what
    /// differs.
    pub compared: Option<(RevisionId, &'a [Difference])>,
}

impl Loaded<'_> {
    /// The running revision.
    #[must_use]
    pub fn running(&self) -> Option<RevisionId> {
        self.standing.current.as_ref().map(|mark| mark.revision)
    }

    /// The revision a new change is saved on, as a form sends it.
    #[must_use]
    pub fn base(&self) -> String {
        self.standing
            .head
            .map(|head| head.to_string())
            .unwrap_or_default()
    }

    /// The number the next saved revision takes, as the save button names
    /// it.
    #[must_use]
    pub fn next(&self) -> String {
        RevisionId::after(self.standing.head).map_or_else(
            || String::from("a new revision"),
            |next| format!("revision {next}"),
        )
    }
}

/// One tab of the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    /// The members.
    Members,
    /// The onward credentials and binding sections.
    Credentials,
    /// The stored queries.
    Queries,
    /// The revisions, newest first.
    History,
    /// The export and import of a configuration as TOML.
    Exchange,
}

impl Tab {
    /// Every tab, in the order the page shows them.
    pub const ALL: [Self; 5] = [
        Self::Members,
        Self::Credentials,
        Self::Queries,
        Self::History,
        Self::Exchange,
    ];

    /// The tab's path.
    #[must_use]
    pub const fn href(self) -> &'static str {
        match self {
            Self::Members => "/configuration/members",
            Self::Credentials => "/configuration/credentials",
            Self::Queries => "/configuration/stored-queries",
            Self::History => "/configuration/history",
            Self::Exchange => "/configuration/import-export",
        }
    }

    /// What the tab says.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Members => "Members",
            Self::Credentials => "Credentials and bindings",
            Self::Queries => "Stored queries",
            Self::History => "History",
            Self::Exchange => "Import and export",
        }
    }

    /// The id of the tab on the page.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Members => "tab-members",
            Self::Credentials => "tab-credentials",
            Self::Queries => "tab-stored-queries",
            Self::History => "tab-history",
            Self::Exchange => "tab-import-export",
        }
    }
}

/// Which part of the page a path names, read from its [`Screen`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shown {
    /// The members.
    Members,
    /// The form that adds a member.
    MemberNew,
    /// A member's form, or a read-only member's detail, by its `node_id`.
    Member(String),
    /// The credentials and binding sections.
    Credentials,
    /// The form that adds onward credentials.
    CredentialNew,
    /// The form of one endpoint's onward credentials.
    Credential(String),
    /// The form that adds a binding section.
    BindingNew,
    /// The form of one binding section.
    Binding(String),
    /// The stored queries.
    Queries,
    /// The form that stores a stored-query version.
    QueryNew,
    /// The history, comparing one revision with the running one.
    History(Option<RevisionId>),
    /// The export and the import.
    Exchange,
}

impl Shown {
    /// The part `screen` names, `None` for a path that names none.
    #[must_use]
    pub fn of(screen: &Screen) -> Option<Self> {
        let item = screen.item.trim();
        Some(match (screen.tab.trim(), item) {
            ("" | "members", "") => Self::Members,
            ("members", "new") => Self::MemberNew,
            ("members", node) => Self::Member(node.to_owned()),
            ("credentials", "") => Self::Credentials,
            ("credentials", "new") => Self::CredentialNew,
            ("credentials", endpoint) => Self::Credential(endpoint.to_owned()),
            ("bindings", "new") => Self::BindingNew,
            ("bindings", section) if !section.is_empty() => Self::Binding(section.to_owned()),
            ("stored-queries", "") => Self::Queries,
            ("stored-queries", "new") => Self::QueryNew,
            ("history", "") => Self::History(screen.compare.trim().parse().ok()),
            ("import-export", "") => Self::Exchange,
            _ => return None,
        })
    }

    /// The tab the part sits under.
    #[must_use]
    pub const fn tab(&self) -> Tab {
        match self {
            Self::Members | Self::MemberNew | Self::Member(_) => Tab::Members,
            Self::Credentials
            | Self::CredentialNew
            | Self::Credential(_)
            | Self::BindingNew
            | Self::Binding(_) => Tab::Credentials,
            Self::Queries | Self::QueryNew => Tab::Queries,
            Self::History(_) => Tab::History,
            Self::Exchange => Tab::Exchange,
        }
    }

    /// Whether the part shows the history.
    #[must_use]
    pub const fn reads_history(&self) -> bool {
        matches!(self, Self::History(_))
    }

    /// The revision the history compares with the running one.
    #[must_use]
    pub const fn compared(&self) -> Option<RevisionId> {
        match self {
            Self::History(compared) => *compared,
            _ => None,
        }
    }
}

/// The page of a gateway whose configuration is not managed: no form.
#[must_use]
pub fn unmanaged(replica: &ReplicaSettings) -> ConfigView {
    let said = format!(
        "Replica {} serves no configuration revisions: its configuration is not managed (config.ui = \"off\"), so the file alone configures it and the console changes nothing.",
        replica.name
    );
    ConfigView {
        header: top::title(()).to_html(),
        main: view! { <p role="status">{said}</p> }.to_html(),
        ..ConfigView::default()
    }
}

/// The part of the configuration page `shown` names, drawn from `loaded`.
///
/// # Errors
/// Returns [`Refusal::Invalid`] for a member, a credential or a binding
/// section the gateway does not hold.
pub fn page(loaded: &Loaded<'_>, shown: &Shown) -> Result<ConfigView, Refusal> {
    let (lead, main) = match shown {
        Shown::Members => (String::new(), members::tab(loaded).to_html()),
        Shown::MemberNew => (String::new(), members::member_form(loaded, None).to_html()),
        Shown::Member(node) => (String::new(), members::member(loaded, node)?.to_html()),
        Shown::Credentials => (String::new(), credentials::tab(loaded).to_html()),
        Shown::CredentialNew => (
            String::new(),
            credentials::credential_form(loaded, None)?.to_html(),
        ),
        Shown::Credential(endpoint) => (
            String::new(),
            credentials::credential_form(loaded, Some(endpoint))?.to_html(),
        ),
        Shown::BindingNew => (
            String::new(),
            bindings::binding_form(loaded, None)?.to_html(),
        ),
        Shown::Binding(section) => (
            String::new(),
            bindings::binding_form(loaded, Some(section))?.to_html(),
        ),
        Shown::Queries => (String::new(), queries::tab(loaded).to_html()),
        Shown::QueryNew => (String::new(), queries::form_page(loaded).to_html()),
        Shown::History(_) => (
            exchange::compared(loaded).to_html(),
            exchange::history(loaded).to_html(),
        ),
        Shown::Exchange => (
            exchange::export(loaded).to_html(),
            exchange::import(loaded).to_html(),
        ),
    };
    let tab = shown.tab();
    Ok(ConfigView {
        header: top::header(loaded).to_html(),
        pending: top::pending(loaded).to_html(),
        tabs: tabs(tab).to_html(),
        tab: tab.id().to_owned(),
        lead,
        main,
    })
}

/// The tabs, the one at `current` selected, as the WAI-ARIA tabs pattern
/// marks them: the arrow keys move between them and Enter opens one.
fn tabs(current: Tab) -> impl IntoView {
    let links = Tab::ALL
        .into_iter()
        .map(|tab| {
            let selected = tab == current;
            view! {
                <a
                    href=tab.href()
                    id=tab.id()
                    role="tab"
                    class="fui-tab"
                    aria-selected=if selected { "true" } else { "false" }
                    aria-controls="config-panel"
                    tabindex=if selected { "0" } else { "-1" }
                >
                    {tab.label()}
                </a>
            }
        })
        .collect_view();
    view! {
        <div role="tablist" aria-label="Configuration" class="fui-tabs" data-tabs="">
            {links}
        </div>
    }
}

/// Who `author` is, in words.
pub(super) fn author(author: &Author) -> String {
    match author {
        Author::File => String::from("the configuration file"),
        Author::Operator { subject, .. } => format!("operator {subject}"),
        Author::DevelopmentLoopback => {
            String::from("a loopback peer under the development profile")
        }
        Author::Gateway => String::from("the gateway, as no operator confirmed in time"),
        _ => String::from("an author this console does not know"),
    }
}

/// The running revision's mark, in words.
pub(super) fn mark_said(mark: &Current) -> String {
    let outcome = if mark.outcome == Outcome::PendingRestart {
        format!(
            " Some of its settings take effect only at a restart: {}.",
            mark.needs_restart.join(", ")
        )
    } else {
        String::new()
    };
    format!(
        "Revision {} runs, applied by {} at {}.{outcome}",
        mark.revision,
        author(&mark.applied_by),
        reading(&mark.applied_at.to_string())
    )
}

#[cfg(test)]
mod tests {
    use super::{Screen, Shown, Tab};

    /// The screen of `tab` and `item`.
    fn screen(tab: &str, item: &str) -> Screen {
        Screen {
            tab: tab.to_owned(),
            item: item.to_owned(),
            compare: String::new(),
        }
    }

    #[test]
    fn each_path_names_its_part_and_its_tab() {
        for (tab, item, shown, under) in [
            ("", "", Shown::Members, Tab::Members),
            ("members", "new", Shown::MemberNew, Tab::Members),
            (
                "members",
                "node-a",
                Shown::Member(String::from("node-a")),
                Tab::Members,
            ),
            ("credentials", "new", Shown::CredentialNew, Tab::Credentials),
            (
                "bindings",
                "pixm",
                Shown::Binding(String::from("pixm")),
                Tab::Credentials,
            ),
            ("stored-queries", "new", Shown::QueryNew, Tab::Queries),
            ("history", "", Shown::History(None), Tab::History),
            ("import-export", "", Shown::Exchange, Tab::Exchange),
        ] {
            let read = Shown::of(&screen(tab, item));
            assert_eq!(Some(&shown), read.as_ref(), "{tab}/{item}");
            assert_eq!(under, shown.tab());
        }
        for (tab, item) in [("revisions", ""), ("history", "3"), ("bindings", "")] {
            assert_eq!(None, Shown::of(&screen(tab, item)), "{tab}/{item}");
        }
    }
}
