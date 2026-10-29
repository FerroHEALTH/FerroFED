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
use ferrohealth_ui::widget::action::{answer_field, answered};
use leptos::form::ActionForm;
use leptos::prelude::*;

use crate::act::clear::{ClearEhrId, ClearLine, ClearOutcome, EVERY_REPLICA};
use crate::act::{Acted, ReplicaAnswer};
use crate::app::INTEGRITY;
use crate::locale::{Msg, tr, tr_with};
use crate::pages::{refused_view, relative, stopped};
use crate::read::incidents::{Incident, Incidents};

/// The id of the clear's dialog.
const CLEAR_DIALOG: &str = "clear-dialog";

/// How many incidents are open, `None` when the gateway did not answer.
#[cfg(not(target_arch = "wasm32"))]
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

/// Integrity.
#[must_use]
pub fn page() -> AnyView {
    let resolved = crate::pages::query_param("show").as_deref() == Some("resolved");
    let clear = ServerAction::<ClearEhrId>::new();
    // NOTE: no specification governs this: our own design; the clear's answer is drawn
    // at the top and its dialog opens empty again, so no posted field is drawn back.
    let _posted = answered(&clear);
    crate::read::follow(clear);
    let read = Resource::new(crate::read::changes, |_| {
        crate::read::incidents::incidents_now()
    });
    view! {
        {crate::app::title(tr(Msg::IntegrityTitle))}
        <Suspense fallback=|| ()>
            {move || {
                let outcome = clear
                    .value()
                    .get()
                    .map(|answer| answer.unwrap_or_else(ClearOutcome::from));
                Suspend::new(async move { integrity(read.await, resolved, outcome, clear) })
            }}
        </Suspense>
    }
    .into_any()
}

/// The page of `read`, on the Resolved tab when `resolved`, with the
/// clear's `outcome` at the top after a post, its form calling `clear`.
fn integrity(
    read: Result<Incidents, crate::read::Stop>,
    resolved: bool,
    outcome: Option<ClearOutcome>,
    clear: ServerAction<ClearEhrId>,
) -> AnyView {
    let replicas = crate::visit::visit_untracked().replicas();
    let held = read.as_ref().map(Incidents::held).unwrap_or_default();
    let clear = (!replicas.is_empty()).then(|| clear_form(&replicas, &held, clear));
    let heading = page_heading(
        Header {
            title: tr(Msg::IntegrityTitle).into(),
            subtitle: Some(tr(Msg::IntegritySubtitle).into()),
            ..Header::default()
        },
        ActionGroup::none(),
    )
    .into_any();
    let answer = outcome.map(answer);
    let body = match read {
        Err(stop) => stopped(&stop),
        Ok(incidents) => tabs(&incidents, resolved),
    };
    view! {
        {heading}
        {clear}
        {stack(
            view! {
                {answer}
                {body}
            },
        )}
    }
    .into_any()
}

/// The clear's form in its dialog: the `ehr_id`, offered from the open
/// incidents' `held`, and the replica among `replicas` or every one.
fn clear_form(replicas: &[String], held: &[String], clear: ServerAction<ClearEhrId>) -> AnyView {
    let known = held
        .iter()
        .map(|ehr_id| view! { <option value=ehr_id.clone()></option> })
        .collect_view();
    let choices = replicas
        .iter()
        .map(|name| view! { <option value=name.clone()>{name.clone()}</option> })
        .collect_view();
    let ehr_id = Field::new("clear-ehr-id", tr(Msg::ClearEhrIdLabel))
        .help(tr(Msg::ClearEhrIdHelp))
        .holds_id()
        .view(view! {
            <input
                type="text"
                id="clear-ehr-id"
                name="ehr_id"
                class=INPUT
                list="clear-ehr-ids"
                autocomplete="off"
                spellcheck="false"
                required
                value=held.first().cloned().unwrap_or_default()
            />
            <datalist id="clear-ehr-ids">{known}</datalist>
        });
    let replica = Field::new("clear-replica", tr(Msg::ClearReplicaLabel)).view(view! {
        <select id="clear-replica" name="replica" class=SELECT>
            <option value=EVERY_REPLICA>{tr(Msg::ClearEveryReplica)}</option>
            {choices}
        </select>
    });
    let body = view! {
        <ActionForm action=clear attr:class="fui-form">
            {answer_field()}
            {ehr_id}
            {replica}
            <p class="fui-small fui-muted">{tr(Msg::ClearRecorded)}</p>
            <input type="hidden" name=crate::act::clear::CONFIRM value="yes" />
            <div class="fui-dialog-actions">
                <button
                    type="button"
                    class=Variant::Secondary.class()
                    command="close"
                    commandfor=CLEAR_DIALOG
                >
                    {tr(Msg::Cancel)}
                </button>
                <button type="submit" class=Variant::Primary.class()>
                    {tr(Msg::ClearSubmit)}
                </button>
            </div>
        </ActionForm>
    }
    .into_any();
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
fn answer(outcome: ClearOutcome) -> AnyView {
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
    let lines = answers.iter().map(replica_line).collect_view();
    view! {
        {load_report(LoadReport {
            outcome: report,
            arrival: Arrival::AfterAction,
            title: tr(
                    match report {
                        Outcome::Ok => Msg::ClearDone,
                        Outcome::Warn => Msg::ClearPartly,
                        Outcome::Bad => Msg::ClearNotDone,
                    },
                )
                .into_any(),
            counts: Vec::new(),
            action: None,
        })}
        <ul class="fui-stack fui-small">{lines}</ul>
    }
    .into_any()
}

/// What one replica did with the clear: what it held, or its refusal
/// with its status and code, or that it gave no answer.
fn replica_line(answer: &ReplicaAnswer<ClearLine>) -> AnyView {
    let said = crate::configure::said::replica_said(answer, "clear", cleared_line);
    view! { <li>{said}</li> }.into_any()
}

/// What `replica` held before it cleared the `ehr_id`, as `line` reports.
fn cleared_line(replica: &str, line: &ClearLine) -> AnyView {
    let held = match line.held {
        HeldBefore::Nothing => tr(Msg::ClearHeldNothing),
        _ => tr_with(
            Msg::ClearHeldAt,
            &[("claimants", line.claimants.join(", ").into())],
        ),
    };
    tr_with(
        Msg::ClearReplicaDone,
        &[
            ("replica", replica.to_owned().into()),
            ("held", held.into()),
        ],
    )
    .into_any()
}

/// The binder tabs Open and Resolved over their incidents, and the counts
/// of each kind.
fn tabs(incidents: &Incidents, resolved: bool) -> AnyView {
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
        view! {
            <p class="fui-panel-body fui-muted">
                {tr(if resolved { Msg::IntegrityNoneResolved } else { Msg::IntegrityNoneOpen })}
            </p>
        }
        .into_any()
    } else {
        incident_table(shown, resolved)
    };
    let counts: Vec<AnyView> = incidents
        .counts
        .iter()
        .map(|(kind, count)| {
            view! {
                <tr>
                    {title_cell(
                        None,
                        what(kind),
                        Some(view! { <code>{kind.clone()}</code> }.into_any()),
                    )} <td class="fui-num">{ferrohealth_ui::i18n::number::grouped(*count)}</td>
                </tr>
            }
            .into_any()
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
    let counts_table = (!counts.is_empty()).then(|| {
        data_table(DataTable {
            caption: &tr(Msg::IntegrityCountsTitle),
            columns: &counts_columns,
            sorting: Sorting::default(),
            body: Body::Rows(counts.collect_view().into_any()),
        })
    });
    let panel = panel_sections(view! {
        {panel_section(None, list)}
        {counts_table
            .map(|table| panel_section(
                Some(SectionHead {
                    title: tr(Msg::IntegrityCountsTitle),
                    count: None,
                    actions: None,
                }),
                table,
            ))}
    });
    binder(
        tr(Msg::IntegrityTitle),
        vec![open_tab, resolved_tab],
        if resolved { "resolved" } else { "open" },
        panel,
    )
}

/// The table of `incidents`, with who cleared each when `resolved`.
fn incident_table(incidents: &[Incident], resolved: bool) -> AnyView {
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
    let rows = incidents
        .iter()
        .map(|incident| {
            let sub = view! { <code>{incident.kind.clone()}</code> }.into_any();
            let ehr_id = incident
                .ehr_id
                .clone()
                .or_else(|| incident.creating_system_id.clone())
                .map_or_else(
                    || tr(Msg::None).into_any(),
                    |id| view! { <code>{id}</code> }.into_any(),
                );
            let when = match (&incident.cleared, resolved) {
                (Some(clear), true) => relative(&clear.at),
                _ => relative(&incident.at),
            };
            view! {
                <tr>
                    {title_cell(None, what(&incident.kind), Some(sub))} {cell(&columns[1], ehr_id)}
                    {cell(&columns[2], incident.nodes.join(", "))} {cell(&columns[3], when)}
                </tr>
            }
        })
        .collect_view()
        .into_any();
    data_table(DataTable {
        caption: &tr(if resolved {
            Msg::IntegrityResolved
        } else {
            Msg::IntegrityOpen
        }),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    })
    .into_any()
}
