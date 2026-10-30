// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Integrity: the incidents the replica keeps.
//!
//! The binder tabs Open and Resolved carry their counts; each incident is in
//! plain words with its kind as a hint, its `ehr_id`, its nodes and when.
//! Under them sit the counts of each kind since the replica started, and the
//! clear of a held `ehr_id` where an admin listener is configured.
//!
//! The tab is the query's `show`, which names no patient; the clear posts its
//! fields, so no `ehr_id` reaches a URL.

use ferrofed_registry::operator::HeldBefore;
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Arg;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::form::{Field, INPUT, SELECT};
use ferrohealth_ui::view::load_report::{Arrival, LoadReport, Outcome, load_report};
use ferrohealth_ui::view::popover::{PopoverForm, popover_form};
use ferrohealth_ui::view::surface::{
    Header, SectionHead, page_heading, panel_section, panel_sections, stack,
};
use ferrohealth_ui::view::tabs::{Tab, binder};

use crate::act::clear::{ClearLine, ClearOutcome, EVERY_REPLICA};
use crate::act::fields::Fields;
use crate::act::{Acted, ReplicaAnswer};
use crate::app::INTEGRITY;
use crate::locale::{Msg, current, tr, tr_with};
use crate::pages::{h, opt, refused_view, relative, stopped, t};
use crate::read::incidents::{Incident, Incidents};

/// The id of the clear's dialog.
const CLEAR_DIALOG: &str = "clear-dialog";

/// How many incidents are open, `None` when the gateway did not answer.
pub async fn open_count() -> Option<u32> {
    let (state, token) = crate::read::signed_in().ok()?;
    // NOTE: no specification governs this: our own design; the count is the shell's,
    // so a refusal is logged as the status read, and the navigation says it is unread.
    match state.gateway().incidents(&token).await {
        Ok(report) => {
            let incidents = Incidents::of(&report);
            Some(u32::try_from(incidents.open.len()).unwrap_or(u32::MAX))
        }
        Err(error) => {
            let _stop = crate::read::stopped(&error, crate::read::Asked::Status);
            None
        }
    }
}

/// What an incident of `kind` means, in plain words; the kind itself for
/// one the console does not know.
#[must_use]
pub fn what(kind: &str) -> String {
    let msg = match kind {
        "EhrIdCollision" => Msg::KindEhrIdCollision,
        "IndexInsertCollision" => Msg::KindIndexInsertCollision,
        "LearnedCreatingSystemConflict" => Msg::KindLearnedConflict,
        "RegisteredCreatingSystemConflict" => Msg::KindRegisteredConflict,
        "PrescriptionIdentifierCollision" => Msg::KindPrescriptionCollision,
        _ => return kind.to_owned(),
    };
    tr(msg)
}

/// Integrity, with the clear's answer at the top when the clear's form
/// `posted` to it.
pub async fn page(posted: Option<Fields>) -> Html {
    let resolved = crate::pages::query_param("show").as_deref() == Some("resolved");
    // NOTE: no specification governs this: our own design; the clear's answer is drawn
    // at the top and its dialog opens empty again, so no posted field is drawn back.
    let outcome = match posted {
        Some(fields) => Some(crate::act::clear::clear_ehr_id(&fields).await),
        None => None,
    };
    let read = crate::read::incidents::read().await;
    crate::app::title(tr(Msg::IntegrityTitle)).then(integrity(read, resolved, outcome))
}

/// The page of `read`, on the Resolved tab when `resolved`, with the
/// clear's `outcome` at the top after a post.
fn integrity(
    read: Result<Incidents, crate::read::Stop>,
    resolved: bool,
    outcome: Option<ClearOutcome>,
) -> Html {
    let replicas = crate::visit::with(crate::visit::Visit::replicas);
    let held = read.as_ref().map(Incidents::held).unwrap_or_default();
    let clear = (!replicas.is_empty()).then(|| clear_form(&replicas, &held, resolved));
    let heading = page_heading(
        &current(),
        Header {
            title: tr(Msg::IntegrityTitle).into(),
            subtitle: Some(tr(Msg::IntegritySubtitle).into()),
            ..Header::default()
        },
        ActionGroup::none(),
    );
    let answer = outcome.map(answer);
    let body = match read {
        Err(stop) => stopped(&stop),
        Ok(incidents) => tabs(&incidents, resolved),
    };
    Html::join([heading, opt(clear), stack(opt(answer).then(body))])
}

/// The clear's form in its dialog: the `ehr_id`, offered from the open
/// incidents' `held`, and the replica among `replicas` or every one. It
/// posts to Integrity, on the tab in force (`resolved`).
fn clear_form(replicas: &[String], held: &[String], resolved: bool) -> Html {
    let known = Html::join(
        held.iter()
            .map(|ehr_id| h(format!("<option value=\"{}\"></option>", t(ehr_id)))),
    );
    let choices = Html::join(replicas.iter().map(|name| {
        h(format!(
            "<option value=\"{name}\">{name}</option>",
            name = t(name)
        ))
    }));
    let ehr_id = Field::new("clear-ehr-id", tr(Msg::ClearEhrIdLabel))
        .help(tr(Msg::ClearEhrIdHelp))
        .holds_id()
        .view(h(format!(
            "<input type=\"text\" id=\"clear-ehr-id\" name=\"ehr_id\" class=\"{INPUT}\" \
             list=\"clear-ehr-ids\" autocomplete=\"off\" spellcheck=\"false\" required \
             value=\"{}\"><datalist id=\"clear-ehr-ids\">{known}</datalist>",
            t(held.first().cloned().unwrap_or_default())
        )));
    let replica = Field::new("clear-replica", tr(Msg::ClearReplicaLabel)).view(h(format!(
        "<select id=\"clear-replica\" name=\"replica\" class=\"{SELECT}\">\
         <option value=\"{EVERY_REPLICA}\">{}</option>{choices}</select>",
        t(tr(Msg::ClearEveryReplica))
    )));
    let action = if resolved {
        format!("{INTEGRITY}?show=resolved")
    } else {
        String::from(INTEGRITY)
    };
    let body = h(format!(
        "<form method=\"post\" action=\"{}\" class=\"fui-form\">{ehr_id}{replica}\
         <p class=\"fui-small fui-muted\">{}</p>\
         <input type=\"hidden\" name=\"{}\" value=\"yes\">\
         <div class=\"fui-dialog-actions\">\
         <button type=\"button\" class=\"{}\" command=\"close\" commandfor=\"{CLEAR_DIALOG}\">{}</button>\
         <button type=\"submit\" class=\"{}\">{}</button></div></form>",
        t(action),
        t(tr(Msg::ClearRecorded)),
        crate::act::clear::CONFIRM,
        Variant::Secondary.class(),
        t(tr(Msg::Cancel)),
        Variant::Primary.class(),
        t(tr(Msg::ClearSubmit)),
    ));
    popover_form(PopoverForm {
        id: CLEAR_DIALOG.into(),
        variant: Variant::Primary,
        glyph: Glyph::ROTATE_CCW,
        label: tr(Msg::ClearAction).into(),
        title: tr(Msg::ClearTitle).into(),
        body,
    })
}

/// The answer of a clear: one line per replica, or why no replica was
/// asked.
fn answer(outcome: ClearOutcome) -> Html {
    let answers = match outcome {
        ClearOutcome::Refused(refused) => return refused_view(&refused),
        ClearOutcome::Failed(fault) => return crate::pages::fault_view(&fault),
        ClearOutcome::Answered(answers) => answers,
    };
    let failed = answers
        .iter()
        .filter(|answer| !matches!(answer.acted, Acted::Done(_)))
        .count();
    let report = if failed == 0 {
        Outcome::Ok
    } else if failed == answers.len() {
        Outcome::Bad
    } else {
        Outcome::Warn
    };
    let lines = Html::join(answers.iter().map(replica_line));
    load_report(LoadReport {
        outcome: report,
        arrival: Arrival::AfterAction,
        title: t(tr(match report {
            Outcome::Ok => Msg::ClearDone,
            Outcome::Warn => Msg::ClearPartly,
            Outcome::Bad => Msg::ClearNotDone,
        })),
        counts: Vec::new(),
        action: None,
    })
    .then(h(format!("<ul class=\"fui-stack fui-small\">{lines}</ul>")))
}

/// What one replica did with the clear: what it held, or its refusal
/// with its status and code, or that it gave no answer.
fn replica_line(answer: &ReplicaAnswer<ClearLine>) -> Html {
    let said = crate::configure::said::replica_said(answer, "clear", cleared_line);
    h(format!("<li>{said}</li>"))
}

/// What `replica` held before it cleared the `ehr_id`, as `line` reports.
fn cleared_line(replica: &str, line: &ClearLine) -> Html {
    let held = match line.held {
        HeldBefore::Nothing => tr(Msg::ClearHeldNothing),
        _ => tr_with(
            Msg::ClearHeldAt,
            &[("claimants", line.claimants.join(", ").into())],
        ),
    };
    t(tr_with(
        Msg::ClearReplicaDone,
        &[
            ("replica", replica.to_owned().into()),
            ("held", held.into()),
        ],
    ))
}

/// The binder tabs Open and Resolved over their incidents, and the counts
/// of each kind.
fn tabs(incidents: &Incidents, resolved: bool) -> Html {
    let count = |n: usize| u64::try_from(n).unwrap_or(u64::MAX);
    let open_tab = Tab::new("open", INTEGRITY, tr(Msg::IntegrityOpen)).count(
        count(incidents.open.len()),
        tr_with(
            Msg::IntegrityOpenCount,
            &[("count", Arg::from(incidents.open.len()))],
        ),
    );
    let resolved_tab = Tab::new(
        "resolved",
        format!("{INTEGRITY}?show=resolved"),
        tr(Msg::IntegrityResolved),
    )
    .count(
        count(incidents.resolved.len()),
        tr_with(
            Msg::IntegrityResolvedCount,
            &[("count", Arg::from(incidents.resolved.len()))],
        ),
    );
    let shown = if resolved {
        &incidents.resolved
    } else {
        &incidents.open
    };
    let list = if shown.is_empty() {
        h(format!(
            "<p class=\"fui-panel-body fui-muted\">{}</p>",
            t(tr(if resolved {
                Msg::IntegrityNoneResolved
            } else {
                Msg::IntegrityNoneOpen
            }))
        ))
    } else {
        incident_table(shown, resolved)
    };
    let counts: Vec<Html> = incidents
        .counts
        .iter()
        .map(|(kind, count)| {
            h(format!(
                "<tr>{} <td class=\"fui-num\">{}</td></tr>",
                title_cell(
                    None,
                    t(what(kind)),
                    Some(h(format!("<code>{}</code>", t(kind)))),
                ),
                t(ferrohealth_ui::i18n::number::grouped(&current(), *count)),
            ))
        })
        .collect();
    let counts_columns = [
        Column {
            role: Role::Title,
            ..Column::text("kind", tr(Msg::ColumnKind))
        },
        Column {
            numeric: true,
            ..Column::text("count", tr(Msg::ColumnSinceStart))
        },
    ];
    let locale = current();
    let counts_table = (!counts.is_empty()).then(|| {
        data_table(DataTable {
            caption: &tr(Msg::IntegrityCountsTitle),
            columns: &counts_columns,
            sorting: Sorting::default(),
            body: Body::Rows(Html::join(counts)),
        })
    });
    let panel = panel_sections(
        panel_section(&locale, None, list).then(opt(counts_table.map(|table| {
            panel_section(
                &locale,
                Some(SectionHead {
                    title: tr(Msg::IntegrityCountsTitle),
                    count: None,
                    actions: None,
                }),
                table,
            )
        }))),
    );
    binder(
        &locale,
        tr(Msg::IntegrityTitle),
        vec![open_tab, resolved_tab],
        if resolved { "resolved" } else { "open" },
        panel,
    )
}

/// The table of `incidents`, with who cleared each when `resolved`.
fn incident_table(incidents: &[Incident], resolved: bool) -> Html {
    let columns = [
        Column {
            role: Role::Title,
            ..Column::text("what", tr(Msg::ColumnWhatHappened))
        },
        Column {
            mono: false,
            ..Column::text("ehr_id", tr(Msg::ColumnEhrId))
        },
        Column::text("nodes", tr(Msg::ColumnNodes)),
        Column::text(
            "when",
            tr(if resolved {
                Msg::ColumnCleared
            } else {
                Msg::ColumnDetected
            }),
        ),
    ];
    let rows = incidents.iter().map(|incident| {
        let sub = h(format!("<code>{}</code>", t(&incident.kind)));
        let ehr_id = incident
            .ehr_id
            .clone()
            .or_else(|| incident.creating_system_id.clone())
            .map_or_else(
                || t(tr(Msg::None)),
                |id| h(format!("<code>{}</code>", t(id))),
            );
        let when = match (&incident.cleared, resolved) {
            (Some(clear), true) => relative(&clear.at),
            _ => relative(&incident.at),
        };
        h(format!(
            "<tr>{} {}{} {}</tr>",
            title_cell(None, t(what(&incident.kind)), Some(sub)),
            cell(&columns[1], ehr_id),
            cell(&columns[2], t(incident.nodes.join(", "))),
            cell(&columns[3], when),
        ))
    });
    data_table(DataTable {
        caption: &tr(if resolved {
            Msg::IntegrityResolved
        } else {
            Msg::IntegrityOpen
        }),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(Html::join(rows)),
    })
}
