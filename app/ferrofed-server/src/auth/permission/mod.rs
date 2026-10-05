// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What each operation of the façade requires of its caller: one table over
//! every ITS-REST operation, and the scope check (§13.1, N25; ITS-REST SMART
//! on openEHR, master08 §Resource Scopes).
//!
//! SMART on openEHR is cited from the ITS-REST Release-1.1.0 source vendored
//! at `docs/specs/its-rest/docs/smart_app_launch/` (`master07-authorization.adoc`
//! and `master08-scopes.adoc`), a document whose `manifest_vars.adoc` declares
//! it DEVELOPMENT status in that release: a draft the release does not
//! stabilise.
//!
//! The SMART on openEHR grammar defines three resource families,
//! `template-`, `composition-` and `aql-`, each with CRUDS permissions. An
//! operation on one of them needs a granted resource scope of that family,
//! in the `user/` or `system/` compartment, whose permissions hold the
//! operation's and whose pattern covers the resource. The other resources
//! of an EHR (the EHR itself, its `EHR_STATUS`, its `DIRECTORY` and its
//! CONTRIBUTIONs) have no family of their own, and are held to the
//! `composition-` family with the operation's permission, over every
//! template. A `patient/` grant is confined to the patient of the token's
//! launch context (master07 §Context Selection), an `ehrId` that means
//! nothing outside the platform that issued it (§12.5). It grants nothing
//! unless the deployment binds its issuer to that platform's member, and
//! then only on an EHR's data: a `composition-` grant, or an `aql-` search,
//! confined to the patient the gateway resolves the `ehrId` to (§5.2). The
//! DEMOGRAPHIC API has no
//! family either: only a client the issuer's entry lists as a demographic
//! client reaches it. The admin operations, and any operation the table does
//! not list, are refused to every caller. Where the
//! gateway cannot see which resource a request addresses (an ad hoc query,
//! a composition whose template only the node knows, an upload whose
//! template is in its body), only a pattern covering every resource, `*` or
//! `**`, covers it, as a specific pattern cannot be shown to match. A
//! `system/aql-*` grant counts only for a backend client the issuer's entry
//! lists, because it "would grant access to all registered and ad-hoc AQL
//! queries system-wide" (master08 §Resource Scopes). A scope the grammar
//! reads as anything but a resource scope grants nothing.
//!
//! [`table`] holds what each operation requires, and [`grant`] whether a
//! caller's scopes grant it.

pub mod grant;
pub mod table;

use openehr_its::rest::routes::RouteMatch;
use openehr_sdt::smart_scopes::{Permission, ResourceFamily};

/// What an operation requires of its caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Requirement {
    /// A verified caller and nothing more: the federation's self-description
    /// and the answers the gateway gives itself without reaching a node.
    Caller,
    /// A verified caller with a purpose of use whose client the issuer's
    /// entry lists as a demographic client: the DEMOGRAPHIC API, which no
    /// SMART on openEHR resource family covers.
    Demographic,
    /// No caller: an admin operation, or one the table does not list.
    Refused,
    /// A verified caller whose token carries the operator scope its issuer's
    /// entry names: the read-only operator surface, `{base}/operator/`,
    /// which no SMART on openEHR resource family covers. No purpose of use
    /// is asked, because the surface returns no clinical data.
    Operator,
    /// A verified caller with a purpose of use and a resource scope that
    /// grants `permission` on the resource.
    Scope {
        /// The resource family.
        family: ResourceFamily,
        /// The CRUDS permission.
        permission: Permission,
        /// Which resource the request addresses.
        resource: Resource,
    },
}

/// Which resource of its family a request addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Resource {
    /// One the gateway cannot name: only a pattern covering every resource
    /// covers it.
    Unnamed,
    /// The one the path parameter of this name names.
    Path(&'static str),
}

/// What the operation `matched` names requires, or `None` for an operation
/// the table does not list.
#[must_use]
pub fn of(matched: &RouteMatch) -> Option<Requirement> {
    table::TABLE
        .iter()
        .find(|(group, operation, _)| *group == matched.group && *operation == matched.operation_id)
        .map(|&(_, _, requirement)| requirement)
}
