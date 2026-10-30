// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The top of every tab: the page header with the revision that runs and
//! the one saved, and the card of what the saved revision changes, with its
//! apply, and of the confirmation the running revision awaits, with its
//! confirm and its roll back (§13.1 onward credentials, commit-confirm).

use ferrofed_registry::revision::{Current, Review, RevisionId, Signed};
use ferrohealth_ui::icon::{Glyph, icon};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::dialog::{Explanation, info_dialog};
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::view::status::pill;
use ferrohealth_ui::html::Html;
use ferrohealth_ui::view::countdown::{Countdown, countdown};
use ferrohealth_ui::view::time::{reading, timestamp};

use crate::configure::render::parts::{Text, confirmed, diff_table, form, hidden, lede, text};
use crate::configure::render::{Loaded, ROLLBACK_REASON, author};
use crate::locale::{Msg, current, tr, tr_with};
use crate::pages::{h, opt, t};

/// The page's title and its one line with the mode badge, and `pills` in
/// the revision line under it (T3).
pub(super) fn title(pills: Html) -> Html {
    let pills = String::from(pills);
    let badge = ferrohealth_ui::view::tag::Tag::new(tr(Msg::ModeManaged))
        .glyph(Glyph::MONITOR)
        .view();
    Html::join([
        ferrohealth_ui::view::surface::page_heading(
            &current(),
            ferrohealth_ui::view::surface::Header {
                title: tr(Msg::Configuration).into(),
                subtitle: Some(tr(Msg::GatewaySSettingsRevisionsEachChange).into()),
                badge: Some(badge),
                ..ferrohealth_ui::view::surface::Header::default()
            },
            ferrohealth_ui::view::action::ActionGroup::none(),
        ),
        ferrohealth_ui::view::template::settings::revision_line(h(format!(
            "{pills}<button type=\"button\" class=\"{}\" command=\"show-modal\" \
             commandfor=\"{HOW}\" aria-haspopup=\"dialog\">{}{}</button>",
            Variant::Ghost.class(),
            icon(Glyph::INFO, "16"),
            t(tr(Msg::ConfigHowTitle)),
        ))),
        how(),
    ])
}

/// The id of the dialog that explains how the configuration works.
const HOW: &str = "config-how";

/// The dialog that explains how the configuration works: what stays in
/// the file, what a revision holds, and how to stop managing revisions.
fn how() -> Html {
    info_dialog(
        &current(),
        Explanation {
            id: HOW.into(),
            title: tr(Msg::ConfigHowTitle).into(),
        },
        h(format!(
            "<p>{}</p><h3>{}</h3><p class=\"fui-small fui-muted\">{}</p><h3>{}</h3>\
             <p class=\"fui-small fui-muted\">{}</p><p class=\"fui-small fui-muted\">{}</p>\
             <p class=\"fui-small fui-muted\">{}</p>",
            t(tr(Msg::ConfigHowLead)),
            t(tr(Msg::ConfigHowCoreTitle)),
            t(tr(Msg::ConfigHowCore)),
            t(tr(Msg::ConfigHowRestTitle)),
            t(tr(Msg::ConfigHowRest)),
            t(tr(Msg::ConfigHowOwners)),
            t(tr(Msg::ConfigHowOff)),
        )),
    )
}

/// The page header: the revision that runs and, when one is saved and not
/// applied, that one, each a pill that says so in words.
pub(super) fn header(loaded: &Loaded<'_>) -> Html {
    let running = loaded.running();
    let runs = match running {
        Some(running) => pill(
            Tone::Ok,
            &tr_with(
                Msg::RevisionRunning2,
                &[("running", (running).to_string().into())],
            ),
        ),
        None => pill(Tone::Neutral, &tr(Msg::NoRevisionRuns)),
    };
    let saved = loaded
        .standing
        .head
        .filter(|head| Some(*head) != running)
        .map(|head| {
            pill(
                Tone::Warn,
                &tr_with(
                    Msg::RevisionSavedNotApplied,
                    &[("head", (head).to_string().into())],
                ),
            )
        });
    title(runs.then(opt(saved)))
}

/// The rollback of the running revision `mark` to the one before it, a
/// confirmation step asking for the reason.
pub(super) fn rollback(mark: &Current, slot: &'static str) -> Html {
    let Some(previous) = mark.previous else {
        return Html::empty();
    };
    let running = mark.revision;
    // NOTE: no specification governs this: our own design; the history and the
    // pending card can both offer the roll back, so each keeps ids of its own.
    let (dialog, reason_id) = if slot == "main" {
        ("rollback-confirm", ROLLBACK_REASON)
    } else {
        ("rollback-confirm-pending", "cfg-rollback-reason-pending")
    };
    form(
        "",
        Html::join([
            hidden("action", String::from("rollback")),
            hidden("current", running.to_string()),
            confirmed(
                dialog,
                (Glyph::ROTATE_CCW, tr(Msg::RollBack), Variant::Danger),
                (
                    tr_with(
                        Msg::RollBackRevisionContentRevisionSaved,
                        &[
                            ("running", (running).to_string().into()),
                            ("previous", (previous).to_string().into()),
                        ],
                    ),
                    tr(Msg::RollBack),
                ),
                text(
                    (reason_id, "form[reason]"),
                    tr(Msg::Reason),
                    String::new(),
                    Text {
                        required: true,
                        ..Text::default()
                    },
                ),
            ),
        ]),
    )
}

/// The whole seconds from `from` to `to`, none when `to` is not after it.
fn seconds_between(from: jiff::Timestamp, to: jiff::Timestamp) -> u32 {
    u32::try_from(to.duration_since(from).as_secs()).unwrap_or(0)
}

/// The countdown (FerroUI F2) of a revision applied at `applied` that the
/// gateway rolls back at `deadline` unless an operator confirms it: the
/// seconds left as the server draws the page, which FerroUI's `countdown`
/// module counts down.
fn commit_countdown(applied: jiff::Timestamp, deadline: jiff::Timestamp) -> Html {
    let until = deadline.strftime("%H:%M:%S").to_string();
    countdown(Countdown::new(
        &current(),
        "confirm-countdown",
        seconds_between(jiff::Timestamp::now(), deadline),
        seconds_between(applied, deadline),
        &until,
    ))
}

/// What the running revision `mark` awaits: its countdown, its confirm and
/// its roll back.
fn awaiting(mark: &Current) -> Html {
    let Some(awaiting) = &mark.awaiting else {
        return Html::empty();
    };
    let revision = mark.revision;
    let changed = awaiting.resources.join(", ");
    let by = timestamp(&current(), &awaiting.deadline.to_string());
    let confirm = form(
        "",
        Html::join([
            hidden("action", String::from("confirm")),
            hidden("revision", revision.to_string()),
            confirmed(
                "confirm-confirm",
                (Glyph::CHECK, tr(Msg::Confirm), Variant::Primary),
                (
                    tr_with(
                        Msg::ConfirmRevisionGatewayKeepsStopsCountdown,
                        &[("revision", (revision).to_string().into())],
                    ),
                    tr(Msg::Confirm),
                ),
                Html::empty(),
            ),
        ]),
    );
    h(format!(
        "<div class=\"fui-split\"><div><h2 id=\"awaiting-title\" class=\"fui-card-title\">{}</h2>{}</div>{}</div>\
         <p>{}{}{}{by}{}</p><div class=\"fui-cluster\">{confirm} {}</div>",
        t(tr_with(
            Msg::RevisionAwaitsYourConfirmation,
            &[("revision", (revision).to_string().into())],
        )),
        lede(tr(Msg::ConfirmOnceGatewayWorksRollBack), "commit-confirm"),
        commit_countdown(mark.applied_at, awaiting.deadline),
        t(tr(Msg::ItChanged)),
        t(changed),
        t(tr(Msg::UnlessOperatorConfirms)),
        t(tr(Msg::GatewayRollsBack)),
        rollback(mark, "pending"),
    ))
}

/// Who signed `signed`, and when, in words.
fn signed_said(signed: &Signed) -> String {
    tr_with(
        Msg::At2,
        &[
            ("value0", (author(&signed.by)).clone().into()),
            ("value1", reading(&current(), &signed.at.to_string()).into()),
        ],
    )
}

/// What `review` says the saved revision takes before and after it applies,
/// and the approval when it still takes one.
fn review(review: &Review) -> Html {
    let confirms = (!review.confirms.is_empty()).then(|| {
        tr_with(
            Msg::OnceAppliedAwaitsConfirmationChanges,
            &[("review", (review.confirms.join(", ")).clone().into())],
        )
    });
    let approval = (!review.approval.is_empty()).then(|| {
        let approvals = if review.approvals.is_empty() {
            tr(Msg::NoneYet)
        } else {
            review
                .approvals
                .iter()
                .map(signed_said)
                .collect::<Vec<_>>()
                .join("; ")
        };
        tr_with(
            Msg::ChangesWhichTakeApprovalOperatorOther,
            &[
                ("review", (review.approval.join(", ")).clone().into()),
                ("value1", (author(&review.author)).clone().into()),
                ("approvals", (approvals).clone().into()),
            ],
        )
    });
    let approve = (!review.approval.is_empty() && !review.approved).then(|| {
        let revision = review.revision;
        form(
            "",
            Html::join([
                hidden("action", String::from("approve")),
                hidden("revision", revision.to_string()),
                confirmed(
                    "approve-confirm",
                    (
                        Glyph::CHECK,
                        tr_with(
                            Msg::ApproveRevision,
                            &[("revision", (revision).to_string().into())],
                        ),
                        Variant::Secondary,
                    ),
                    (
                        tr_with(
                            Msg::ApproveRevisionOperatorOtherThanAuthor,
                            &[("revision", (revision).to_string().into())],
                        ),
                        tr(Msg::Approve),
                    ),
                    Html::empty(),
                ),
            ]),
        )
    });
    Html::join([
        opt(confirms.map(|said| notice(Tone::Info, Announce::Status, None, t(said)))),
        opt(approval.map(|said| notice(Tone::Info, Announce::Status, None, t(said)))),
        opt(approve),
    ])
}

/// The saved revision `head` that does not run: what it changes against
/// the running one, and its apply.
fn saved(loaded: &Loaded<'_>, head: RevisionId) -> Html {
    let changes = loaded.pending.unwrap_or_default();
    let count = match changes.len() {
        1 => tr(Msg::N1Setting),
        many => tr_with(Msg::Settings3, &[("many", (many).to_string().into())]),
    };
    let heading = if loaded.pending.is_some() {
        tr_with(
            Msg::RevisionChanges,
            &[
                ("head", (head).to_string().into()),
                ("count", (count).clone().into()),
            ],
        )
    } else {
        tr_with(
            Msg::RevisionSavedNotApplied2,
            &[("head", (head).to_string().into())],
        )
    };
    let table = loaded
        .pending
        .map(|changes| diff_table(changes, (loaded.running(), head)));
    let review = loaded
        .standing
        .review
        .as_ref()
        .filter(|review| review.revision == head)
        .map(review);
    let apply = form(
        "",
        Html::join([
            hidden("action", String::from("apply")),
            hidden("revision", head.to_string()),
            confirmed(
                "apply-confirm",
                (
                    Glyph::PLAY,
                    tr_with(Msg::ApplyRevision, &[("head", (head).to_string().into())]),
                    Variant::Primary,
                ),
                (
                    tr_with(
                        Msg::ApplyRevisionRunningGatewayEveryReplica,
                        &[("head", (head).to_string().into())],
                    ),
                    tr(Msg::Apply),
                ),
                Html::empty(),
            ),
        ]),
    );
    h(format!(
        "<div class=\"fui-split\"><div><h2 id=\"pending-title\" class=\"fui-card-title\">{}</h2>{}</div>\
         <div class=\"fui-cluster\">{apply}</div></div>{}{}",
        t(heading),
        lede(tr(Msg::SavedNotAppliedReadWhatChanges), "what-a-revision-holds"),
        opt(table),
        opt(review),
    ))
}

/// The card at the top of every tab while a revision is saved and not
/// applied, or the running one awaits its confirmation; nothing otherwise.
pub(super) fn pending(loaded: &Loaded<'_>) -> Html {
    let mark = loaded.standing.current.as_ref();
    let awaits = mark.is_some_and(|mark| mark.awaiting.is_some());
    let head = loaded
        .standing
        .head
        .filter(|head| Some(*head) != loaded.running());
    if !awaits && head.is_none() {
        return Html::empty();
    }
    let first = mark.map(awaiting);
    let second = head.map(|head| saved(loaded, head));
    let between = (awaits && head.is_some()).then(|| h(String::from("<hr>")));
    Html::join([opt(first), opt(between), opt(second)])
}
