// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The identity lifecycle over PMIR (track 8 of §16.3, Annex A.4).
//!
//! It holds what an ITI-93 message means for the resolution bindings, and
//! the ITI-94 subscriber that asks for the messages (§5.2, "PMIR for
//! identity lifecycle").
//!
//! A message changes routing state only. [`change_of`] reads which `ehr_id`s
//! its changes touch, from the identifiers a Patient Master Identity carries
//! in a member's `ehr_id` domain (Annex A.1, the assigning authority whose
//! identifier values are that member's `ehr_id`s), and returns the
//! [`IdentityChange`] the bindings' hook applies. No identifier value leaves
//! here except as an `ehr_id` the bindings already key on.

use std::collections::BTreeSet;

use ferrofed_registry::id::EhrId;
use ferrohealth_smart::client::outbound::OutboundClient;
use ihe_iti::pmir::PmirSubscriber;
use ihe_iti::pmir::error::InvalidInput;
use ihe_iti::pmir::feed::{Event, Feed, PatientIdentity};
use secrecy::ExposeSecret;
use thiserror::Error;

use crate::session::IdentityChange;

/// What one change touches.
enum Touched {
    /// Nothing a binding could name.
    Nothing,
    /// These `ehr_id`s.
    Ehrs(BTreeSet<EhrId>),
    /// Something the change cannot scope.
    Unscoped,
}

/// The change `feed` makes to the resolution bindings, where `domains` are
/// the members' `ehr_id` domains, or `None` when it makes none.
///
/// - A create touches no binding: no binding can name an identity that did
///   not exist.
/// - An update states the identity as it now is and never what it lost
///   (§2:3.93.4.1.2.3), so it cannot be scoped and every binding goes.
/// - A merge or a delete touches the `ehr_id`s its Patient carries in a
///   member's domain (§2:3.93.4.1.2.4); one that carries none, or one whose
///   value there is not an `ehr_id`, cannot be scoped.
///
/// A message with any change that cannot be scoped is
/// [`IdentityChange::Unscoped`]; otherwise the `ehr_id`s every change
/// touches, in order.
#[must_use]
pub fn change_of(feed: &Feed, domains: &BTreeSet<String>) -> Option<IdentityChange> {
    let mut ehrs = BTreeSet::new();
    for event in feed.events() {
        let touched = match event {
            Event::Created(_) => Touched::Nothing,
            Event::Merged { subsumed, .. } => carried(subsumed, domains),
            Event::Deleted(identity) => carried(identity, domains),
            // NOTE: PMIR §2:3.93.4.1.2.3, an update states only the identity it now
            // is, so it is unscoped, as is any change kind this reader does not know.
            _ => Touched::Unscoped,
        };
        match touched {
            Touched::Nothing => {}
            Touched::Ehrs(touched) => ehrs.extend(touched),
            Touched::Unscoped => return Some(IdentityChange::Unscoped),
        }
    }
    (!ehrs.is_empty()).then(|| IdentityChange::Ehrs(ehrs.into_iter().collect()))
}

/// The `ehr_id`s `identity` carries in `domains`, or [`Touched::Unscoped`]
/// when it carries none or one that is not an `ehr_id`.
fn carried(identity: &PatientIdentity, domains: &BTreeSet<String>) -> Touched {
    let mut ehrs = BTreeSet::new();
    for identifier in identity.identifiers() {
        if !identifier
            .system()
            .is_some_and(|system| domains.contains(system))
        {
            continue;
        }
        match EhrId::new(identifier.value().expose_secret()) {
            Ok(ehr_id) => {
                ehrs.insert(ehr_id);
            }
            Err(_not_an_ehr_id) => return Touched::Unscoped,
        }
    }
    if ehrs.is_empty() {
        Touched::Unscoped
    } else {
        Touched::Ehrs(ehrs)
    }
}

/// A subscriber that cannot be built.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PmirConfigError {
    /// The Registry's base URL is not an `http(s)` URL without a query.
    #[error(
        "the Patient Identity Registry base URL is not an http(s) URL without a query or a fragment"
    )]
    Base(#[source] InvalidInput),
}

/// The ITI-94 subscriber of the Registry `client`'s endpoint table names.
///
/// It authenticates with the credential and the TLS material of `client`,
/// by which the Registry authorizes the subscription (§2:3.94.5).
///
/// # Errors
/// [`PmirConfigError::Base`] for a base that is no `http(s)` URL without a
/// query or a fragment.
pub fn subscriber(client: OutboundClient) -> Result<PmirSubscriber, PmirConfigError> {
    PmirSubscriber::new(client).map_err(PmirConfigError::Base)
}
