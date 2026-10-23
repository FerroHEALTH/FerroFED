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
use ferrohealth_ui::view::time::{reading, timestamp};
use leptos::prelude::*;

use crate::configure::render::parts::{Text, confirmed, diff_table, form, hidden, lede, text};
use crate::configure::render::{Loaded, ROLLBACK_REASON, author};
use crate::locale::{Msg, tr, tr_with};

/// The page's title and its one line with the mode badge, and `pills` in
/// the revision line under it (T3).
pub(super) fn title(pills: impl IntoView + 'static) -> impl IntoView {
    let badge = ferrohealth_ui::view::tag::Tag::new(tr(Msg::ModeManaged))
        .glyph(Glyph::MONITOR)
        .view();
    view! {
        {ferrohealth_ui::view::surface::page_heading(
            ferrohealth_ui::view::surface::Header {
                title: tr(Msg::Configuration).into(),
                subtitle: Some(tr(Msg::GatewaySSettingsRevisionsEachChange).into()),
                badge: Some(badge),
                ..ferrohealth_ui::view::surface::Header::default()
            },
            ferrohealth_ui::view::action::ActionGroup::none(),
        )}
        {ferrohealth_ui::view::template::settings::revision_line(
            view! {
                {pills}
                <button
                    type="button"
                    class=Variant::Ghost.class()
                    command="show-modal"
                    commandfor=HOW
                    aria-haspopup="dialog"
                >
                    {icon(Glyph::INFO, "16")}
                    {tr(Msg::ConfigHowTitle)}
                </button>
            }
                .into_any(),
        )}
        {how()}
    }
}

/// The id of the dialog that explains how the configuration works.
const HOW: &str = "config-how";

/// The dialog that explains how the configuration works: what stays in
/// the file, what a revision holds, and how to stop managing revisions.
fn how() -> impl IntoView {
    info_dialog(
        Explanation {
            id: HOW.into(),
            title: tr(Msg::ConfigHowTitle).into(),
        },
        view! {
            <p>{tr(Msg::ConfigHowLead)}</p>
            <h3>{tr(Msg::ConfigHowCoreTitle)}</h3>
            <p class="fui-small fui-muted">{tr(Msg::ConfigHowCore)}</p>
            <h3>{tr(Msg::ConfigHowRestTitle)}</h3>
            <p class="fui-small fui-muted">{tr(Msg::ConfigHowRest)}</p>
            <p class="fui-small fui-muted">{tr(Msg::ConfigHowOwners)}</p>
            <p class="fui-small fui-muted">{tr(Msg::ConfigHowOff)}</p>
        },
    )
}

/// The page header: the revision that runs and, when one is saved and not
/// applied, that one, each a pill that says so in words.
pub(super) fn header(loaded: &Loaded<'_>) -> impl IntoView + use<> {
    let running = loaded.running();
    let runs = match running {
        Some(running) => pill(
            Tone::Ok,
            &tr_with(
                Msg::RevisionRunning2,
                &[("running", (running).to_string().into())],
            ),
        )
        .into_any(),
        None => pill(Tone::Neutral, &tr(Msg::NoRevisionRuns)).into_any(),
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
    title(view! {
        {runs}
        {saved}
    })
}

/// The rollback of the running revision `mark` to the one before it, a
/// confirmation step asking for the reason.
pub(super) fn rollback(mark: &Current, slot: &'static str) -> AnyView {
    let Some(previous) = mark.previous else {
        return ().into_any();
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
        view! {
            {hidden("action", String::from("rollback"))}
            {hidden("current", running.to_string())}
            {confirmed(
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
            )}
        },
    )
    .into_any()
}

/// The time left until `deadline`, in milliseconds since the epoch, as the
/// countdown starts it; the page's script counts it down.
fn left(deadline: i64) -> String {
    let now = jiff::Timestamp::now().as_millisecond();
    crate::configure::countdown(deadline.saturating_sub(now))
}

/// What the running revision `mark` awaits: its countdown, its confirm and
/// its roll back.
fn awaiting(mark: &Current) -> AnyView {
    let Some(awaiting) = &mark.awaiting else {
        return ().into_any();
    };
    let revision = mark.revision;
    let deadline = awaiting.deadline.as_millisecond();
    let changed = awaiting.resources.join(", ");
    let by = timestamp(&awaiting.deadline.to_string());
    view! {
        <div class="fui-split">
            <div>
                <h2 id="awaiting-title" class="fui-card-title">
                    {tr_with(
                        Msg::RevisionAwaitsYourConfirmation,
                        &[("revision", (revision).to_string().into())],
                    )}
                </h2>
                {lede(tr(Msg::ConfirmOnceGatewayWorksRollBack), "commit-confirm")}
            </div>
            <span class="fui-pill fui-tone-warn" role="timer" aria-live="off">
                <span id="confirm-countdown" data-deadline=deadline.to_string()>
                    {left(deadline)}
                </span>
            </span>
        </div>
        <p>
            {tr(Msg::ItChanged)}{changed}{tr(Msg::UnlessOperatorConfirms)}{by}
            {tr(Msg::GatewayRollsBack)}
        </p>
        <div class="fui-cluster">
            {form(
                "",
                view! {
                    {hidden("action", String::from("confirm"))}
                    {hidden("revision", revision.to_string())}
                    {confirmed(
                        "confirm-confirm",
                        (Glyph::CHECK, tr(Msg::Confirm), Variant::Primary),
                        (
                            tr_with(
                                Msg::ConfirmRevisionGatewayKeepsStopsCountdown,
                                &[("revision", (revision).to_string().into())],
                            ),
                            tr(Msg::Confirm),
                        ),
                        (),
                    )}
                },
            )} {rollback(mark, "pending")}
        </div>
    }
    .into_any()
}

/// Who signed `signed`, and when, in words.
fn signed_said(signed: &Signed) -> String {
    tr_with(
        Msg::At2,
        &[
            ("value0", (author(&signed.by)).clone().into()),
            ("value1", (reading(&signed.at.to_string())).clone().into()),
        ],
    )
}

/// What `review` says the saved revision takes before and after it applies,
/// and the approval when it still takes one.
fn review(review: &Review) -> AnyView {
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
            view! {
                {hidden("action", String::from("approve"))}
                {hidden("revision", revision.to_string())}
                {confirmed(
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
                    (),
                )}
            },
        )
    });
    view! {
        {confirms.map(|said| { notice(Tone::Info, Announce::Status, None, said) })}
        {approval.map(|said| { notice(Tone::Info, Announce::Status, None, said) })}
        {approve}
    }
    .into_any()
}

/// The saved revision `head` that does not run: what it changes against
/// the running one, and its apply.
fn saved(loaded: &Loaded<'_>, head: RevisionId) -> AnyView {
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
        view! {
            {hidden("action", String::from("apply"))}
            {hidden("revision", head.to_string())}
            {confirmed(
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
                (),
            )}
        },
    );
    view! {
        <div class="fui-split">
            <div>
                <h2 id="pending-title" class="fui-card-title">
                    {heading}
                </h2>
                {lede(tr(Msg::SavedNotAppliedReadWhatChanges), "what-a-revision-holds")}
            </div>
            <div class="fui-cluster">{apply}</div>
        </div>
        {table}
        {review}
    }
    .into_any()
}

/// The card at the top of every tab while a revision is saved and not
/// applied, or the running one awaits its confirmation; nothing otherwise.
pub(super) fn pending(loaded: &Loaded<'_>) -> AnyView {
    let mark = loaded.standing.current.as_ref();
    let awaits = mark.is_some_and(|mark| mark.awaiting.is_some());
    let head = loaded
        .standing
        .head
        .filter(|head| Some(*head) != loaded.running());
    if !awaits && head.is_none() {
        return ().into_any();
    }
    let first = mark.map(awaiting);
    let second = head.map(|head| saved(loaded, head));
    let between = (awaits && head.is_some()).then(|| view! { <hr /> });
    view! {
        {first}
        {between}
        {second}
    }
    .into_any()
}
