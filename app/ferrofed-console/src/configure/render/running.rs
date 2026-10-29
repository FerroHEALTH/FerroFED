// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The settings tabs (T3) of the configuration page.
//!
//! Each shows the deployment-wide settings of one section of the running
//! configuration, one row each with its value, where the value comes from,
//! its key and its Next start mark, and the tab's Edit, whose form saves a
//! revision with the settings the operator changed.
//!
//! The rows are drawn from the replica's running configuration
//! (`GET /admin/config/running`), never from a revision's text, so a
//! secret is named by the file it is read from and never by a value.

use ferrofed_registry::running::{RunningConfig, Section, Setting, Source};
use ferrohealth_ui::icon::{Glyph, icon};
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::form::{INPUT, SELECT, described_by};
use ferrohealth_ui::view::template::settings::{
    Setting as Row, ValueSource, setting_row, settings_panel,
};
use leptos::prelude::*;

use crate::configure::render::Loaded;
use crate::configure::render::parts::{error_line, form, hidden, reason, save_bar};
use crate::locale::{Msg, tr, tr_with};

/// `key` with each array position written `[]`, as the book's tables name
/// a key of every `[[trust.issuer]]`.
fn generic(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    let mut skipping = false;
    for character in key.chars() {
        match character {
            '[' => {
                skipping = true;
                out.push('[');
            }
            ']' => {
                skipping = false;
                out.push(']');
            }
            _ if skipping => {}
            other => out.push(other),
        }
    }
    out
}

/// The label and the one line of the setting `key`.
fn words(key: &str) -> (Msg, Msg) {
    match generic(key).as_str() {
        "federation.per_node_timeout_ms" => {
            (Msg::SettingPerNodeTimeout, Msg::SettingPerNodeTimeoutDesc)
        }
        "federation.overall_timeout_ms" => {
            (Msg::SettingOverallTimeout, Msg::SettingOverallTimeoutDesc)
        }
        "federation.node_selection" => (Msg::SettingNodeSelection, Msg::SettingNodeSelectionDesc),
        "federation.best_effort" => (Msg::SettingBestEffort, Msg::SettingBestEffortDesc),
        "federation.localization.on_failure" => {
            (Msg::SettingLocalizerFails, Msg::SettingLocalizerFailsDesc)
        }
        "federation.localization.timeout_ms" => {
            (Msg::SettingLocalizerBudget, Msg::SettingLocalizerBudgetDesc)
        }
        "federation.fan_out_stored_queries" => {
            (Msg::SettingStoredFanOut, Msg::SettingStoredFanOutDesc)
        }
        "federation.fan_out_template_upload" => {
            (Msg::SettingTemplateFanOut, Msg::SettingTemplateFanOutDesc)
        }
        "federation.offset_strategy" => (Msg::SettingPaging, Msg::SettingPagingDesc),
        "federation.max_offset_window" => (Msg::SettingPagingWindow, Msg::SettingPagingWindowDesc),
        "federation.max_node_rows" => (Msg::SettingRowsPerNode, Msg::SettingRowsPerNodeDesc),
        "federation.consent.disclose" => (Msg::SettingDisclose, Msg::SettingDiscloseDesc),
        "federation.consent.emergency" => (Msg::SettingEmergency, Msg::SettingEmergencyDesc),
        "stored_queries.backend" => (Msg::SettingBackend, Msg::SettingBackendDesc),
        "stored_queries.path" => (Msg::SettingStorePath, Msg::SettingStorePathDesc),
        "stored_queries.url" => (Msg::SettingConnection, Msg::SettingConnectionDesc),
        "access_log.retention.years" => (Msg::SettingRetention, Msg::SettingRetentionDesc),
        "access_log.emergency_purpose" => (
            Msg::SettingEmergencyPurposes,
            Msg::SettingEmergencyPurposesDesc,
        ),
        "access_log.templates" => (Msg::SettingTemplateMap, Msg::SettingTemplateMapDesc),
        "access_log.archetypes" => (Msg::SettingArchetypeMap, Msg::SettingArchetypeMapDesc),
        "fhir.base" => (Msg::SettingFhirBase, Msg::SettingFhirBaseDesc),
        "fhir.document_url_lifetime_secs" => (Msg::SettingUrlLifetime, Msg::SettingUrlLifetimeDesc),
        "fhir.receive" => (Msg::SettingReceived, Msg::SettingReceivedDesc),
        "auth.audience" => (Msg::SettingAudience, Msg::SettingAudienceDesc),
        "auth.product_scope" => (Msg::SettingProductScope, Msg::SettingProductScopeDesc),
        "auth.issuer" | "auth.issuer[].issuer" => {
            (Msg::SettingTokenIssuer, Msg::SettingTokenIssuerDesc)
        }
        "auth.issuer[].jwks_file" => (Msg::SettingKeySetFile, Msg::SettingKeySetFileDesc),
        "trust.issuer[].issuer" => (Msg::SettingTrustedIssuer, Msg::SettingTrustedIssuerDesc),
        "trust.issuer[].client_secret" => (
            Msg::SettingIntrospectionSecret,
            Msg::SettingIntrospectionSecretDesc,
        ),
        "trust.issuer[].national_contact_point.family_name" => {
            (Msg::SettingNcpFamilyName, Msg::SettingNcpFamilyNameDesc)
        }
        "trust.issuer[].national_contact_point.given_name" => {
            (Msg::SettingNcpGivenName, Msg::SettingNcpGivenNameDesc)
        }
        "trust.issuer[].national_contact_point.country_code" => {
            (Msg::SettingNcpCountryCode, Msg::SettingNcpCountryCodeDesc)
        }
        "trust.issuer[].national_contact_point.professional_issuing_authority" => (
            Msg::SettingNcpProfessionalAuthority,
            Msg::SettingNcpProfessionalAuthorityDesc,
        ),
        "trust.issuer[].national_contact_point.provider_issuing_authority" => (
            Msg::SettingNcpProviderAuthority,
            Msg::SettingNcpProviderAuthorityDesc,
        ),
        "trust.issuer[].national_contact_point.provider_address" => (
            Msg::SettingNcpProviderAddress,
            Msg::SettingNcpProviderAddressDesc,
        ),
        "trust.issuer[].national_contact_point.correlation_header" => {
            (Msg::SettingNcpCorrelation, Msg::SettingNcpCorrelationDesc)
        }
        "trust.issuer[].national_contact_point.additional_countries" => {
            (Msg::SettingNcpCountries, Msg::SettingNcpCountriesDesc)
        }
        _ => (Msg::SettingUnknown, Msg::SettingUnknown),
    }
}

/// The words of the tab of `section`, and its intro.
#[must_use]
pub const fn section_words(section: Section) -> (Msg, Msg) {
    match section {
        Section::Federation => (Msg::SettingsTabFederation, Msg::SettingsIntroFederation),
        Section::FanOut => (Msg::SettingsTabFanOut, Msg::SettingsIntroFanOut),
        Section::Consent => (Msg::SettingsTabConsent, Msg::SettingsIntroConsent),
        Section::StoredQueries => (Msg::StoredQueries, Msg::SettingsIntroStoredQueries),
        Section::AccessLog => (Msg::SettingsTabAccessLog, Msg::SettingsIntroAccessLog),
        Section::Authentication => (
            Msg::SettingsTabAuthentication,
            Msg::SettingsIntroAuthentication,
        ),
        _ => (Msg::SettingsTabFhir, Msg::SettingsIntroFhir),
    }
}

/// The path of the tab of `section`.
#[must_use]
pub const fn tab_href(section: Section) -> &'static str {
    match section {
        Section::Federation => "/configuration/federation",
        Section::FanOut => "/configuration/fan-out",
        Section::Consent => "/configuration/consent",
        Section::StoredQueries => "/configuration/stored-queries",
        Section::AccessLog => "/configuration/access-log",
        Section::Authentication => "/configuration/authentication",
        _ => "/configuration/fhir",
    }
}

/// The path of the edit of the tab of `section`.
#[must_use]
pub fn edit_href(section: Section) -> String {
    format!("/configuration/{}/edit", section.as_str())
}

/// The unit a number of the setting `key` is written with.
fn unit(key: &str) -> Option<Msg> {
    if key.ends_with("_ms") {
        Some(Msg::SettingsMs)
    } else if key.ends_with("_secs") {
        Some(Msg::SettingsSeconds)
    } else if key == "access_log.retention.years" {
        Some(Msg::SettingsYears)
    } else if key.ends_with("_rows") || key.ends_with("_window") {
        Some(Msg::SettingsRows)
    } else {
        None
    }
}

/// `value`, the value of the setting `key`, in words.
fn shown(key: &str, value: &toml::Value) -> AnyView {
    match value {
        toml::Value::Integer(_) | toml::Value::Boolean(_) => plain(key, value).into_any(),
        toml::Value::String(text) => {
            view! { <code class="fui-break">{text.clone()}</code> }.into_any()
        }
        toml::Value::Array(items) if items.is_empty() => tr(Msg::SettingsNotSet).into_any(),
        toml::Value::Array(items) => items
            .iter()
            .map(|item| {
                view! {
                    <span class="fui-tag">{tag(item)}</span>
                    " "
                }
            })
            .collect_view()
            .into_any(),
        toml::Value::Table(table) => {
            tr_with(Msg::SettingsMapped, &[("count", table.len().into())]).into_any()
        }
        other => other.to_string().into_any(),
    }
}

/// A single `value` of the setting `key` in words, as text.
fn plain(key: &str, value: &toml::Value) -> String {
    match value {
        toml::Value::Boolean(true) => tr(Msg::SettingsOn),
        toml::Value::Boolean(false) => tr(Msg::SettingsOff),
        toml::Value::Integer(number) => {
            let arg = u64::try_from(*number).map_or_else(|_| number.to_string().into(), Into::into);
            unit(key).map_or_else(
                || number.to_string(),
                |unit| tr_with(unit, &[("value", arg)]),
            )
        }
        toml::Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// One item of a list in words: a string as it is, an entry by its
/// `category` or `code`.
fn tag(item: &toml::Value) -> String {
    match item {
        toml::Value::String(text) => text.clone(),
        toml::Value::Table(table) => table
            .get("category")
            .or_else(|| table.get("code"))
            .and_then(toml::Value::as_str)
            .map_or_else(|| item.to_string(), str::to_owned),
        other => other.to_string(),
    }
}

/// Where the value of `setting` comes from, when it is not where the page
/// says the settings come from: the running revision of a managed
/// configuration, the file of one that is not.
fn marker(setting: &Setting, managed: bool) -> Option<ValueSource> {
    match setting.source {
        Source::Environment => Some(ValueSource::Environment),
        Source::Default => Some(ValueSource::Default),
        Source::File => managed.then_some(ValueSource::File),
        _ => None,
    }
}

/// The value cell of `setting` as it runs: the value or the secret's
/// reference, and the value the next start takes.
fn value_cell(setting: &Setting) -> AnyView {
    let value = match (&setting.secret, &setting.value) {
        (Some(secret), _) if secret.set => match &secret.reference {
            Some(file) => view! {
                <span>
                    {icon(Glyph::LOCK, "12")} {tr(Msg::SettingsSecretSetFile)} " "
                    <code class="fui-break">{file.clone()}</code>
                </span>
            }
            .into_any(),
            None => {
                view! { <span>{icon(Glyph::LOCK, "12")} {tr(Msg::SettingsSecretSetInline)}</span> }
                    .into_any()
            }
        },
        (Some(_), _) | (None, None) => tr(Msg::SettingsNotSet).into_any(),
        (None, Some(value)) => shown(&setting.key, value),
    };
    let next = setting.next.as_ref().map(|next| {
        view! {
            " "
            <span class="fui-small fui-muted">
                {tr_with(Msg::SettingsNextValue, &[("value", plain(&setting.key, next).into())])}
            </span>
        }
    });
    view! {
        {value}
        {next}
    }
    .into_any()
}

/// The rows of `section` as they run.
fn rows(running: &RunningConfig, section: Section) -> Vec<AnyView> {
    let managed = running.ui == "managed";
    running
        .of(section)
        .map(|setting| {
            let (label, description) = words(&setting.key);
            setting_row(
                Row {
                    label: tr(label),
                    description: tr(description),
                    value: value_cell(setting),
                    keys: vec![setting.key.clone()],
                    next_start: setting.restart,
                    source: marker(setting, managed),
                },
                managed,
            )
        })
        .collect()
}

/// The panel of `section` as it runs, with its Edit when the
/// configuration is managed and the section holds an editable setting.
#[must_use]
pub fn panel(running: &RunningConfig, section: Section) -> AnyView {
    let (_, intro) = section_words(section);
    let editable = running.of(section).any(|setting| setting.editable);
    let edit = editable.then(|| {
        view! {
            <a
                href=edit_href(section)
                class=format!("{} fui-button-small", Variant::Secondary.class())
            >
                {icon(Glyph::PENCIL, "16")}
                {tr(Msg::SettingsEdit)}
            </a>
        }
        .into_any()
    });
    settings_panel(tr(intro).into_any(), edit, rows(running, section))
}

/// The tab of `section`, or the line that the running configuration could
/// not be read.
#[must_use]
pub fn tab(loaded: &Loaded<'_>, section: Section) -> AnyView {
    match loaded.running {
        Some(running) => panel(running, section),
        None => unreadable(loaded),
    }
}

/// The line that the running configuration could not be read.
fn unreadable(loaded: &Loaded<'_>) -> AnyView {
    view! {
        <p role="status" class="fui-muted">
            {tr_with(Msg::SettingsUnreadable, &[("replica", loaded.replica.to_owned().into())])}
        </p>
    }
    .into_any()
}

/// The text a value is edited as: a string as it is, anything else as
/// TOML writes it.
fn typed(value: Option<&toml::Value>) -> String {
    match value {
        Some(toml::Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

/// The words a setting of `key` may take, when it takes a few.
fn choices(key: &str) -> Option<&'static [&'static str]> {
    match key {
        "federation.node_selection" => Some(&["localized", "ask-all"]),
        "federation.localization.on_failure" => Some(&["closed", "ask-all"]),
        "federation.offset_strategy" => Some(&["bounded", "reject"]),
        "federation.consent.emergency" => Some(&["apply", "pass-to-node"]),
        "stored_queries.backend" => Some(&["redb", "postgres", "files"]),
        _ => None,
    }
}

/// The control of one editable setting, posted as row `row`.
fn control(row: usize, setting: &Setting, posted: Option<&str>) -> AnyView {
    let id = format!("cfg-setting-{row}");
    let name = format!("form[setting.{row}.value]");
    let held = typed(setting.value.as_ref());
    let current = posted.map_or_else(|| held.clone(), str::to_owned);
    let described = described_by(&id, false, true);
    let label = tr(words(&setting.key).0);
    let input = match (&setting.value, choices(&setting.key)) {
        (Some(toml::Value::Boolean(_)), _) => {
            let options = [("true", Msg::SettingsOn), ("false", Msg::SettingsOff)]
                .into_iter()
                .map(|(value, words)| {
                    view! {
                        <option value=value selected=current == value>
                            {tr(words)}
                        </option>
                    }
                })
                .collect_view();
            view! {
                <select
                    id=id.clone()
                    name=name
                    class=SELECT
                    aria-label=label
                    aria-describedby=described
                >
                    {options}
                </select>
            }
            .into_any()
        }
        (_, Some(words)) => {
            let unset = held.is_empty().then(|| {
                view! {
                    <option value="" selected=current.is_empty()>
                        {tr(Msg::SettingsNotSet)}
                    </option>
                }
            });
            let options = words
                .iter()
                .map(|word| {
                    view! {
                        <option value=*word selected=current == *word>
                            {*word}
                        </option>
                    }
                })
                .collect_view();
            view! {
                <select
                    id=id.clone()
                    name=name
                    class=SELECT
                    aria-label=label
                    aria-describedby=described
                >
                    {unset}
                    {options}
                </select>
            }
            .into_any()
        }
        _ => view! {
            <input
                type="text"
                id=id.clone()
                name=name
                value=current
                class=INPUT
                autocomplete="off"
                spellcheck="false"
                aria-label=label
                aria-describedby=described
            />
        }
        .into_any(),
    };
    view! {
        <input type="hidden" name="form[setting_rows][]" value=row.to_string() />
        <input type="hidden" name=format!("form[setting.{row}.key]") value=setting.key.clone() />
        <input type="hidden" name=format!("form[setting.{row}.was]") value=held />
        {input}
        {error_line(&id)}
    }
    .into_any()
}

/// The edit of the tab of `section`: each setting as a control, a
/// setting that is no single value as it runs, the reason, and the save.
#[must_use]
pub fn edit(loaded: &Loaded<'_>, section: Section) -> AnyView {
    let Some(running) = loaded.running else {
        return unreadable(loaded);
    };
    if running.ui != "managed" {
        return panel(running, section);
    }
    let posted = loaded.drafted(&["save-settings"]);
    let mut row = 0_usize;
    let rows = running
        .of(section)
        .map(|setting| {
            let (label, description) = words(&setting.key);
            let value = if setting.editable {
                let typed_before = posted.and_then(|form| {
                    form.settings
                        .iter()
                        .find(|input| input.key == setting.key)
                        .map(|input| input.value.as_str())
                });
                let drawn = control(row, setting, typed_before);
                row = row.saturating_add(1);
                drawn
            } else {
                view! {
                    {value_cell(setting)}
                    " "
                    <span class="fui-small fui-muted">{tr(Msg::SettingsReadOnlyHere)}</span>
                }
                .into_any()
            };
            setting_row(
                Row {
                    label: tr(label),
                    description: tr(description),
                    value,
                    keys: vec![setting.key.clone()],
                    next_start: setting.restart,
                    source: marker(setting, true),
                },
                true,
            )
        })
        .collect();
    let intro = match loaded.standing.head {
        Some(base) => tr_with(Msg::SettingsIntroEdit, &[("base", base.to_string().into())]),
        None => tr(Msg::SettingsIntroEditFirst),
    };
    view! {
        {form(
            "fui-stack",
            view! {
                {hidden("action", String::from("save-settings"))}
                {hidden("base", loaded.base())}
                {settings_panel(intro.into_any(), None, rows)}
                <p class="fui-help">{tr(Msg::SettingsChangedOnlyHint)}</p>
                {reason()}
                {save_bar(loaded, tab_href(section))}
            },
        )}
    }
    .into_any()
}
