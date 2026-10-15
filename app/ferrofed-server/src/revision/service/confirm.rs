// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The approval and the confirmation of a revision.
//!
//! A revision that changes a resource `config.approval` names takes a second
//! operator's approval before it applies, and one that changes a credential
//! or a binding section takes a confirmation after, without which the
//! gateway rolls it back at its deadline.
//!
//! The deadline is held in the store's running mark, so it holds for every
//! replica sharing the store and across a restart: each replica's watch
//! rolls back a revision past its deadline ([`Revisions::expire`]), the
//! others follow the rollback as any applied revision, and a gateway that
//! starts with one past its deadline rolls it back before it records the
//! file ([`Revisions::expire_unapplied`]). No specification governs this:
//! our own design.

use std::sync::PoisonError;

use ferrofed_registry::revision::{
    Author, Awaiting, Current, Outcome, Review, Revision, RevisionId, Signed,
};
use jiff::Timestamp;

use super::{RevisionError, Revisions, Stale, Unconfirmable, held_table};
use crate::reload::Reloader;
use crate::revision::Swapped;
use crate::revision::policy::{changed, named};

/// The `event` of the record of a confirmation.
pub const CONFIRMED: &str = "config-revision-confirmed";

/// The `event` of the record of an approval.
pub const APPROVED: &str = "config-revision-approved";

/// The `event` of the record of a revision the gateway rolled back
/// unconfirmed.
pub const UNCONFIRMED: &str = "config-unconfirmed-rolled-back";

/// Whether an apply is reviewed for its approval and its confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Reviewed {
    /// An operator's apply: approved first, confirmed after.
    Operator,
    /// A rollback, which restores what ran before and takes neither.
    Rollback,
}

impl Revisions {
    /// What revision `id` takes against the revision that runs: the
    /// resources it changes that take a confirmation, those that take an
    /// approval, and the approvals it holds.
    ///
    /// # Errors
    ///
    /// [`RevisionError::Unknown`] for a revision not held,
    /// [`RevisionError::NoCurrent`] when none runs, and the store's errors.
    pub fn review(&self, id: RevisionId) -> Result<Review, RevisionError> {
        let revision = self.store.revision(id)?.ok_or(RevisionError::Unknown(id))?;
        let running = self.store.current()?.ok_or(RevisionError::NoCurrent)?;
        self.review_against(&revision, &running)
    }

    /// What `revision` takes against the revision `running` marks.
    pub(super) fn review_against(
        &self,
        revision: &Revision,
        running: &Current,
    ) -> Result<Review, RevisionError> {
        let ran = self
            .store
            .revision(running.revision)?
            .ok_or(RevisionError::Unknown(running.revision))?;
        let changes = changed(&held_table(&ran)?, &held_table(revision)?);
        let approval = self.policy.needs_approval(&changes);
        let approvals = self.store.approvals(revision.id)?;
        let approved = approval.is_empty()
            || approvals.iter().any(|approval| {
                matches!(approval.by, Author::Operator { .. })
                    && !approval.by.same_operator(&revision.author)
            });
        Ok(Review {
            revision: revision.id,
            author: revision.author.clone(),
            confirms: named(&changes),
            approval: named(&approval),
            approvals,
            approved,
        })
    }

    /// The confirmation an apply that changes `confirms` awaits, none when
    /// it changes no credential and no binding section.
    pub(super) fn awaiting(
        &self,
        confirms: Vec<String>,
    ) -> Result<Option<Awaiting>, RevisionError> {
        if confirms.is_empty() {
            return Ok(None);
        }
        let deadline = Timestamp::now()
            .checked_add(self.policy.confirm)
            .map_err(RevisionError::Deadline)?;
        Ok(Some(Awaiting {
            deadline,
            resources: confirms,
        }))
    }

    /// Records the approval of revision `id` by `by`, and returns what it
    /// takes now.
    ///
    /// # Errors
    ///
    /// [`RevisionError::Unverified`] when `by` is no verified operator,
    /// [`RevisionError::Unknown`] for a revision not held,
    /// [`RevisionError::ApprovalNotRequired`] for one that changes no
    /// resource `config.approval` names, [`RevisionError::OwnApproval`] when
    /// `by` made it, and the store's errors.
    pub fn approve(&self, id: RevisionId, by: Author) -> Result<Review, RevisionError> {
        if !matches!(by, Author::Operator { .. }) {
            return Err(RevisionError::Unverified);
        }
        let revision = self.store.revision(id)?.ok_or(RevisionError::Unknown(id))?;
        let running = self.store.current()?.ok_or(RevisionError::NoCurrent)?;
        let review = self.review_against(&revision, &running)?;
        if review.approval.is_empty() {
            return Err(RevisionError::ApprovalNotRequired(id));
        }
        if by.same_operator(&revision.author) {
            return Err(RevisionError::OwnApproval(id));
        }
        let approval = Signed {
            by,
            at: Timestamp::now(),
        };
        self.store.approve(id, &approval)?;
        self.review_against(&revision, &running)
    }

    /// Confirms revision `id`, which runs awaiting its confirmation, for
    /// `by` at `now`, and returns the mark the store now holds.
    ///
    /// # Errors
    ///
    /// [`RevisionError::Unverified`] when `by` is no verified operator,
    /// [`RevisionError::NoCurrent`] when no revision runs,
    /// [`RevisionError::Unconfirmable`] when `id` does not run, awaits no
    /// confirmation, or is past its deadline, [`RevisionError::Stale`] when
    /// the mark moved meanwhile, and the store's errors.
    pub fn confirm(
        &self,
        id: RevisionId,
        by: Author,
        now: Timestamp,
    ) -> Result<Current, RevisionError> {
        if !matches!(by, Author::Operator { .. }) {
            return Err(RevisionError::Unverified);
        }
        let _serial = self.serial.lock().unwrap_or_else(PoisonError::into_inner);
        let current = self.store.current()?.ok_or(RevisionError::NoCurrent)?;
        if current.revision != id {
            return Err(Unconfirmable::NotRunning {
                id,
                running: Some(current.revision),
            }
            .into());
        }
        let Some(awaiting) = &current.awaiting else {
            return Err(Unconfirmable::NotAwaiting(id).into());
        };
        if now > awaiting.deadline {
            return Err(Unconfirmable::Expired(id).into());
        }
        let confirmed = Current {
            awaiting: None,
            confirmed: Some(Signed { by, at: now }),
            ..current.clone()
        };
        if let Swapped::Stale { current } = self.store.swap_current(Some(id), &confirmed)? {
            return Err(Stale::Current {
                expected: Some(id),
                current,
            }
            .into());
        }
        Ok(confirmed)
    }

    /// Rolls the running revision back through `reloader` when it awaits a
    /// confirmation whose deadline is before `now`, the gateway its author,
    /// and returns the revision the rollback saved with its mark; none when
    /// nothing is past its deadline.
    ///
    /// # Errors
    ///
    /// Every error of [`Revisions::rollback`]; the unconfirmed revision then
    /// keeps running, and the next call tries again.
    pub fn expire(
        &self,
        reloader: &Reloader,
        now: Timestamp,
    ) -> Result<Option<(Revision, Current)>, RevisionError> {
        let _serial = self.serial.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(current) = self.expired(now)? else {
            return Ok(None);
        };
        let reason = unconfirmed(&current);
        let (revision, mark) = self.rollback_held(reloader, &current, (Author::Gateway, reason))?;
        recorded(current.revision, &revision);
        Ok(Some((revision, mark)))
    }

    /// Moves the running mark off a revision whose confirmation is past its
    /// deadline at `now`, before a start records the file, so the start runs
    /// the revision before it.
    ///
    /// # Errors
    ///
    /// Every error of [`Revisions::rollback`].
    pub fn expire_unapplied(&self, now: Timestamp) -> Result<(), RevisionError> {
        let _serial = self.serial.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(current) = self.expired(now)? {
            self.unconfirmed_back(&current, unconfirmed(&current))?;
        }
        Ok(())
    }

    /// The running mark, when it awaits a confirmation past its deadline at
    /// `now`.
    fn expired(&self, now: Timestamp) -> Result<Option<Current>, RevisionError> {
        Ok(self.store.current()?.filter(|current| {
            current
                .awaiting
                .as_ref()
                .is_some_and(|awaiting| now > awaiting.deadline)
        }))
    }

    /// Saves the revision that ran before `current`, which awaits its
    /// confirmation, as a new revision by the gateway for `reason`, and moves
    /// the mark to it without the reload path: what runs next applies it.
    pub(super) fn unconfirmed_back(
        &self,
        current: &Current,
        reason: String,
    ) -> Result<Revision, RevisionError> {
        let revision = self.restoring(current, (Author::Gateway, reason))?;
        let mark = Current {
            revision: revision.id,
            previous: Some(current.revision),
            outcome: Outcome::Applied,
            needs_restart: Vec::new(),
            applied_by: Author::Gateway,
            applied_at: Timestamp::now(),
            awaiting: None,
            confirmed: None,
        };
        if let Swapped::Stale { current: moved } =
            self.store.swap_current(Some(current.revision), &mark)?
        {
            return Err(Stale::Current {
                expected: Some(current.revision),
                current: moved,
            }
            .into());
        }
        recorded(current.revision, &revision);
        Ok(revision)
    }
}

/// Why the gateway rolls `current` back.
fn unconfirmed(current: &Current) -> String {
    let deadline = current
        .awaiting
        .as_ref()
        .map(|awaiting| awaiting.deadline.to_string())
        .unwrap_or_default();
    format!(
        "revision {} was not confirmed by its deadline {deadline}",
        current.revision
    )
}

/// Records that the gateway rolled revision `unconfirmed` back as
/// `revision`.
fn recorded(unconfirmed: RevisionId, revision: &Revision) {
    tracing::warn!(
        target: crate::facade::security::TARGET,
        event = UNCONFIRMED,
        unconfirmed = %unconfirmed,
        revision = %revision.id,
        "the gateway rolled back a configuration revision no operator confirmed by its deadline"
    );
}
