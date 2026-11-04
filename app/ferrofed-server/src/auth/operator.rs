// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operator a write action on the admin listener was admitted for.
//!
//! The admin listener admits it, and the configuration revisions record it as
//! their author. No specification governs this: our own design.

use crate::auth::caller::Caller;

/// The operator a write action was admitted for.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Operator {
    /// A caller the gate verified, with the operator scope.
    Verified(Box<Caller>),
    /// A loopback peer that sent no credential, admitted under
    /// `profile = "development"` alone.
    DevelopmentLoopback,
}

impl Operator {
    /// The issuer that vouched for the operator, when one did.
    #[must_use]
    pub fn issuer(&self) -> Option<&str> {
        match self {
            Self::Verified(caller) => Some(caller.issuer()),
            Self::DevelopmentLoopback => None,
        }
    }

    /// The operator's subject at that issuer, when one vouched for it.
    #[must_use]
    pub fn subject(&self) -> Option<&str> {
        match self {
            Self::Verified(caller) => Some(caller.subject()),
            Self::DevelopmentLoopback => None,
        }
    }
}
