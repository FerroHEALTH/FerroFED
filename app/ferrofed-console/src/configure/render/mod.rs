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
//! arrives as text, never as markup, a `textarea`'s text among them.

pub mod answers;
mod bindings;
mod credentials;
mod exchange;
mod members;
mod parts;
mod queries;
pub mod running;
mod top;

use ferrofed_registry::console::Resources;
use ferrofed_registry::operator::Page;
use ferrofed_registry::revision::{
    Author, Current, Difference, Outcome, Revision, RevisionId, Standing,
};
use ferrofed_registry::running::{RunningConfig, Section};
use ferrohealth_ui::view::time::reading;
use leptos::prelude::*;

use crate::config::settings::ReplicaSettings;
use crate::configure::model::{ConfigView, Screen};
use crate::locale::{Msg, tr, tr_with};
use crate::read::Refusal;

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
    /// The form as the operator posted it, drawn again in place of what
    /// the gateway holds.
    pub draft: Option<&'a crate::configure::draft::Draft>,
    /// The replica's running configuration, for a settings tab; `None`
    /// when it was not read or could not be.
    pub running: Option<&'a RunningConfig>,
}

impl Loaded<'_> {
    /// The posted form whose action is one of `actions`, to draw again.
    #[must_use]
    pub fn drafted(&self, actions: &[&str]) -> Option<&crate::configure::model::ConfigForm> {
        self.draft
            .map(|draft| &draft.form)
            .filter(|form| actions.contains(&form.action.as_str()))
    }

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
            || tr(Msg::ANewRevision),
            |next| tr_with(Msg::Revision4, &[("next", (next).to_string().into())]),
        )
    }
}

/// One tab of the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    /// How a query fans out to the members.
    Federation,
    /// How a query is paged, and what is sent on.
    FanOut,
    /// What callers learn about consent.
    Consent,
    /// How each access record is classified and kept.
    AccessLog,
    /// The FHIR face.
    Fhir,
    /// Who may call: the trusted issuers, a national contact point among
    /// them, read-only.
    Authentication,
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
    pub const ALL: [Self; 11] = [
        Self::Federation,
        Self::FanOut,
        Self::Consent,
        Self::Queries,
        Self::AccessLog,
        Self::Fhir,
        Self::Authentication,
        Self::Members,
        Self::Credentials,
        Self::History,
        Self::Exchange,
    ];

    /// The tabs of a configuration revisions do not manage: the settings,
    /// read-only.
    pub const SETTINGS: [Self; 7] = [
        Self::Federation,
        Self::FanOut,
        Self::Consent,
        Self::Queries,
        Self::AccessLog,
        Self::Fhir,
        Self::Authentication,
    ];

    /// The tab of the settings of `section`.
    #[must_use]
    pub const fn of(section: Section) -> Self {
        match section {
            Section::Federation => Self::Federation,
            Section::FanOut => Self::FanOut,
            Section::Consent => Self::Consent,
            Section::StoredQueries => Self::Queries,
            Section::AccessLog => Self::AccessLog,
            Section::Authentication => Self::Authentication,
            _ => Self::Fhir,
        }
    }

    /// The section of the settings the tab shows, when it shows some.
    #[must_use]
    pub const fn section(self) -> Option<Section> {
        match self {
            Self::Federation => Some(Section::Federation),
            Self::FanOut => Some(Section::FanOut),
            Self::Consent => Some(Section::Consent),
            Self::Queries => Some(Section::StoredQueries),
            Self::AccessLog => Some(Section::AccessLog),
            Self::Fhir => Some(Section::Fhir),
            Self::Authentication => Some(Section::Authentication),
            Self::Members | Self::Credentials | Self::History | Self::Exchange => None,
        }
    }

    /// The tab's path.
    #[must_use]
    pub const fn href(self) -> &'static str {
        match self.section() {
            Some(section) => running::tab_href(section),
            None => self.other_href(),
        }
    }

    /// The path of a tab that shows no settings.
    const fn other_href(self) -> &'static str {
        match self {
            Self::Members => "/configuration/members",
            Self::Credentials => "/configuration/credentials",
            Self::Federation
            | Self::FanOut
            | Self::Consent
            | Self::Queries
            | Self::AccessLog
            | Self::Fhir
            | Self::Authentication => "/configuration/stored-queries",
            Self::History => "/configuration/history",
            Self::Exchange => "/configuration/import-export",
        }
    }

    /// What the tab says.
    #[must_use]
    pub const fn label(self) -> Msg {
        if let Some(section) = self.section() {
            return running::section_words(section).0;
        }
        match self {
            Self::Federation
            | Self::FanOut
            | Self::Consent
            | Self::Queries
            | Self::AccessLog
            | Self::Fhir
            | Self::Authentication => Msg::StoredQueries,
            Self::Members => Msg::Members,
            Self::Credentials => Msg::CredentialsAndBindings,
            Self::History => Msg::History,
            Self::Exchange => Msg::ImportAndExport,
        }
    }

    /// The id of the tab on the page.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Federation => "tab-federation",
            Self::FanOut => "tab-fan-out",
            Self::Consent => "tab-consent",
            Self::AccessLog => "tab-access-log",
            Self::Fhir => "tab-fhir",
            Self::Authentication => "tab-authentication",
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
    /// The settings of a section, as they run.
    Settings(Section),
    /// The edit of the settings of a section.
    SettingsEdit(Section),
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
            ("", "") => Self::Settings(Section::Federation),
            ("stored-queries", "edit") => Self::SettingsEdit(Section::StoredQueries),
            (tab, "") if tab != "stored-queries" && Section::named(tab).is_some() => {
                Self::Settings(Section::named(tab)?)
            }
            (tab, "edit") if Section::named(tab).is_some() => {
                Self::SettingsEdit(Section::named(tab)?)
            }
            ("members", "") => Self::Members,
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
            Self::Settings(section) | Self::SettingsEdit(section) => Tab::of(*section),
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

    /// Whether the part shows settings of the running configuration.
    #[must_use]
    pub const fn reads_running(&self) -> bool {
        matches!(
            self,
            Self::Settings(_) | Self::SettingsEdit(_) | Self::Queries
        )
    }

    /// The section of settings a configuration revisions do not manage
    /// shows for the part: its own, or the first.
    #[must_use]
    pub const fn file_section(&self) -> Section {
        match self.tab().section() {
            Some(section) => section,
            None => Section::Federation,
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

/// The page of a gateway whose configuration is not managed: the settings
/// of `section` as `running` holds them, read-only.
#[must_use]
pub fn file_only(running: &RunningConfig, section: Section) -> ConfigView {
    let line = view! {
        <p class="fui-small fui-muted">
            {ferrohealth_ui::icon::icon(ferrohealth_ui::icon::Glyph::LOCK, "12")} " "
            {tr(Msg::SettingsFileLine)}
        </p>
    };
    let header = ferrohealth_ui::view::surface::page_heading(
        ferrohealth_ui::view::surface::Header {
            title: tr(Msg::Configuration).into(),
            subtitle: Some(tr(Msg::SettingsFileSubtitle).into()),
            ..ferrohealth_ui::view::surface::Header::default()
        },
        ferrohealth_ui::view::action::ActionGroup::none(),
    );
    ConfigView {
        header: Some(header.into_any()),
        pending: Some(line.into_any()),
        tab: Some(Tab::of(section)),
        lead: None,
        main: Some(running::panel(running, section)),
        file_only: true,
    }
}

/// The page of a gateway whose configuration is not managed: no form.
#[must_use]
pub fn unmanaged(replica: &ReplicaSettings) -> ConfigView {
    let said = tr_with(
        Msg::ReplicaServesNoConfigurationRevisionsConfiguration,
        &[("name", replica.name.clone().into())],
    );
    ConfigView {
        header: Some(top::title(()).into_any()),
        main: Some(view! { <p role="status">{said}</p> }.into_any()),
        ..ConfigView::default()
    }
}

/// The part of the configuration page `shown` names, drawn from `loaded`.
///
/// # Errors
/// Returns [`Refusal::Invalid`] for a member, a credential or a binding
/// section the gateway does not hold.
pub fn page(loaded: &Loaded<'_>, shown: &Shown) -> Result<ConfigView, Refusal> {
    let (lead, main): (Option<AnyView>, AnyView) = match shown {
        Shown::Settings(section) => (None, running::tab(loaded, *section)),
        Shown::SettingsEdit(section) => (None, running::edit(loaded, *section)),
        Shown::Members => (None, members::tab(loaded).into_any()),
        Shown::MemberNew => (None, members::member_form(loaded, None).into_any()),
        Shown::Member(node) => (None, members::member(loaded, node)?.into_any()),
        Shown::Credentials => (None, credentials::tab(loaded).into_any()),
        Shown::CredentialNew => (None, credentials::credential_form(loaded, None)?.into_any()),
        Shown::Credential(endpoint) => (
            None,
            credentials::credential_form(loaded, Some(endpoint))?.into_any(),
        ),
        Shown::BindingNew => (None, bindings::binding_form(loaded, None)?.into_any()),
        Shown::Binding(section) => (
            None,
            bindings::binding_form(loaded, Some(section))?.into_any(),
        ),
        Shown::Queries => (
            Some(running::tab(loaded, Section::StoredQueries)),
            queries::tab(loaded).into_any(),
        ),
        Shown::QueryNew => (None, queries::form_page(loaded).into_any()),
        Shown::History(_) => (
            Some(exchange::compared(loaded).into_any()),
            exchange::history(loaded).into_any(),
        ),
        Shown::Exchange => (
            Some(exchange::export(loaded).into_any()),
            exchange::import(loaded).into_any(),
        ),
    };
    Ok(ConfigView {
        header: Some(top::header(loaded).into_any()),
        pending: Some(top::pending(loaded).into_any()),
        tab: Some(shown.tab()),
        lead,
        main: Some(main),
        file_only: false,
    })
}

/// Who `author` is, in words.
pub(super) fn author(author: &Author) -> String {
    match author {
        Author::File => tr(Msg::TheConfigurationFile),
        Author::Operator { subject, .. } => {
            tr_with(Msg::Operator2, &[("subject", (subject).clone().into())])
        }
        Author::DevelopmentLoopback => tr(Msg::LoopbackPeerUnderDevelopmentProfile),
        Author::Gateway => tr(Msg::GatewayNoOperatorConfirmedTime),
        _ => tr(Msg::AuthorConsoleDoesNotKnow),
    }
}

/// The running revision's mark, in words.
pub(super) fn mark_said(mark: &Current) -> String {
    let outcome = if mark.outcome == Outcome::PendingRestart {
        tr_with(
            Msg::SomeSettingsTakeEffectOnlyRestart,
            &[("mark", (mark.needs_restart.join(", ")).clone().into())],
        )
    } else {
        String::new()
    };
    tr_with(
        Msg::RevisionRunsApplied,
        &[
            ("revision", (mark.revision).to_string().into()),
            ("value1", (author(&mark.applied_by)).clone().into()),
            (
                "value2",
                (reading(&mark.applied_at.to_string())).clone().into(),
            ),
            ("outcome", (outcome).clone().into()),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::{Screen, Section, Shown, Tab};

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
            (
                "",
                "",
                Shown::Settings(Section::Federation),
                Tab::Federation,
            ),
            ("fan-out", "", Shown::Settings(Section::FanOut), Tab::FanOut),
            (
                "access-log",
                "edit",
                Shown::SettingsEdit(Section::AccessLog),
                Tab::AccessLog,
            ),
            (
                "stored-queries",
                "edit",
                Shown::SettingsEdit(Section::StoredQueries),
                Tab::Queries,
            ),
            ("stored-queries", "", Shown::Queries, Tab::Queries),
            ("members", "", Shown::Members, Tab::Members),
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
        for (tab, item) in [
            ("revisions", ""),
            ("history", "3"),
            ("bindings", ""),
            ("fhir", "x"),
        ] {
            assert_eq!(None, Shown::of(&screen(tab, item)), "{tab}/{item}");
        }
    }
}
