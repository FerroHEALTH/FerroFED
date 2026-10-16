// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The top of every tab: the page header with the revision that runs and
//! the one saved, and the card of what the saved revision changes, with its
//! apply, and of the confirmation the running revision awaits, with its
//! confirm and its roll back (§13.1 onward credentials, commit-confirm).

use ferrofed_registry::revision::{Current, Review, RevisionId, Signed};
use leptos::prelude::*;

use crate::configure::render::parts::{Text, confirmed, diff_table, form, hidden, lede, text};
use crate::configure::render::{Loaded, ROLLBACK_REASON, author, when};
use crate::styles::{self, Button};

/// The page's title and its one line, with `pills` at its end.
pub(super) fn title(pills: impl IntoView) -> impl IntoView {
    view! {
        <header class=styles::PAGE_HEADER>
            <div class="min-w-0">
                <h1 class=styles::PAGE_TITLE>"Configuration"</h1>
                <p class=styles::PAGE_SUBTITLE>
                    "The gateway's settings as revisions: each change is saved, then applied."
                </p>
            </div>
            <div class="flex flex-wrap items-center gap-2">{pills}</div>
        </header>
    }
}

/// The page header: the revision that runs and, when one is saved and not
/// applied, that one, each a pill that says so in words.
pub(super) fn header(loaded: &Loaded<'_>) -> impl IntoView + use<> {
    let running = loaded.running();
    let runs = match running {
        Some(running) => {
            view! { <span class="pill tone-ok">{format!("Revision {running} running")}</span> }
                .into_any()
        }
        None => view! { <span class="pill tone-neutral">"No revision runs"</span> }.into_any(),
    };
    let saved = loaded
        .standing
        .head
        .filter(|head| Some(*head) != running)
        .map(|head| {
            view! { <span class="pill tone-warn">{format!("Revision {head} saved, not applied")}</span> }
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
    form(
        slot,
        "inline",
        view! {
            {hidden("action", String::from("rollback"))}
            {hidden("current", running.to_string())}
            {confirmed(
                String::from("rollback-confirm"),
                (String::from("Roll back"), Button::Danger),
                (
                    format!(
                        "Roll back revision {running}? The content of revision {previous} is saved and applied in one step.",
                    ),
                    "Roll back",
                ),
                text(
                    (ROLLBACK_REASON, "form[reason]"),
                    String::from("Reason"),
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
    let said = format!(
        "It changed {}. Unless an operator confirms it by {}, the gateway rolls it back.",
        awaiting.resources.join(", "),
        when(awaiting.deadline)
    );
    view! {
        <div class="flex flex-wrap items-start justify-between gap-3">
            <div class="min-w-0">
                <h2 id="awaiting-title" class="mt-0 mb-1 text-base font-semibold text-ink">
                    {format!("Revision {revision} awaits your confirmation")}
                </h2>
                {lede(
                    "Confirm it once the gateway works with it, or roll it back.",
                    "commit-confirm",
                )}
            </div>
            <span class="pill tone-warn" role="timer" aria-live="off">
                <span id="confirm-countdown" data-deadline=deadline.to_string()>
                    {left(deadline)}
                </span>
            </span>
        </div>
        <p class="mt-0">{said}</p>
        <div class="flex flex-wrap gap-2">
            {form(
                "pending",
                "inline",
                view! {
                    {hidden("action", String::from("confirm"))}
                    {hidden("revision", revision.to_string())}
                    {confirmed(
                        String::from("confirm-confirm"),
                        (String::from("Confirm"), Button::Primary),
                        (
                            format!(
                                "Confirm revision {revision}? The gateway keeps it and stops the countdown.",
                            ),
                            "Confirm",
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
    format!("{} at {}", author(&signed.by), when(signed.at))
}

/// What `review` says the saved revision takes before and after it applies,
/// and the approval when it still takes one.
fn review(review: &Review) -> AnyView {
    let confirms = (!review.confirms.is_empty()).then(|| {
        format!(
            "Once applied, it awaits a confirmation: it changes {}.",
            review.confirms.join(", ")
        )
    });
    let approval = (!review.approval.is_empty()).then(|| {
        let approvals = if review.approvals.is_empty() {
            String::from("none yet")
        } else {
            review
                .approvals
                .iter()
                .map(signed_said)
                .collect::<Vec<_>>()
                .join("; ")
        };
        format!(
            "It changes {}, which take the approval of an operator other than its author, {}. Approved by: {approvals}.",
            review.approval.join(", "),
            author(&review.author)
        )
    });
    let approve = (!review.approval.is_empty() && !review.approved).then(|| {
        let revision = review.revision;
        form(
            "pending",
            "inline",
            view! {
                {hidden("action", String::from("approve"))}
                {hidden("revision", revision.to_string())}
                {confirmed(
                    String::from("approve-confirm"),
                    (format!("Approve revision {revision}"), Button::Secondary),
                    (
                        format!(
                            "Approve revision {revision} as an operator other than its author? The approval names you.",
                        ),
                        "Approve",
                    ),
                    (),
                )}
            },
        )
    });
    view! {
        {confirms
            .map(|said| {
                view! {
                    <p class="mt-0 text-sm" role="status">
                        {said}
                    </p>
                }
            })}
        {approval
            .map(|said| {
                view! {
                    <p class="mt-0 text-sm" role="status">
                        {said}
                    </p>
                }
            })}
        {approve}
    }
    .into_any()
}

/// The saved revision `head` that does not run: what it changes against
/// the running one, and its apply.
fn saved(loaded: &Loaded<'_>, head: RevisionId) -> AnyView {
    let changes = loaded.pending.unwrap_or_default();
    let count = match changes.len() {
        1 => String::from("1 setting"),
        many => format!("{many} settings"),
    };
    let heading = if loaded.pending.is_some() {
        format!("Revision {head} changes {count}")
    } else {
        format!("Revision {head} is saved and not applied")
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
        "pending",
        "inline",
        view! {
            {hidden("action", String::from("apply"))}
            {hidden("revision", head.to_string())}
            {confirmed(
                String::from("apply-confirm"),
                (format!("Apply revision {head}"), Button::Primary),
                (
                    format!(
                        "Apply revision {head} to the running gateway? Every replica follows it.",
                    ),
                    "Apply",
                ),
                (),
            )}
        },
    );
    view! {
        <div class="flex flex-wrap items-start justify-between gap-3">
            <div class="min-w-0">
                <h2 id="pending-title" class="mt-0 mb-1 text-base font-semibold text-ink">
                    {heading}
                </h2>
                {lede(
                    "Saved and not applied. Read what it changes, then apply it.",
                    "what-a-revision-holds",
                )}
            </div>
            <div class="flex flex-wrap items-center gap-2">{apply}</div>
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
    let between = (awaits && head.is_some())
        .then(|| view! { <hr class="my-5 border-0 border-t border-line" /> });
    view! {
        {first}
        {between}
        {second}
    }
    .into_any()
}
