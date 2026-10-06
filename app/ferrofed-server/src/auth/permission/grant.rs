// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Whether a caller's scopes grant what an operation requires.
//!
//! The checks are the operator scope, a SMART on openEHR resource scope of
//! the operation's family, and the confinement of a `patient/` grant to its
//! patient (§13.1, N25; ITS-REST SMART on openEHR, master07 §Context
//! Selection, master08 §Resource Scopes).

use std::collections::BTreeSet;

use openehr_sdt::smart_scopes::{
    Compartment, Permission, ResourceFamily, ResourceScope, ResourceSelector, SmartScope,
};

/// Returns whether `granted` holds the operator scope of the caller's issuer.
///
/// `granted` is the space-separated `scope` claim, and `operator_scope` is
/// matched as one whole token of it (RFC 6749 §3.3); an issuer that names
/// none admits no operator.
#[must_use]
pub fn operator(operator_scope: Option<&str>, granted: &str) -> bool {
    operator_scope.is_some_and(|scope| granted.split(' ').any(|token| token == scope))
}

/// What a caller's issuer entry lets the gateway honour beyond a `user/`
/// grant.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Honoured {
    /// The caller is a backend client the entry lists, so a `system/aql-*`
    /// grant counts.
    pub backend: bool,
    /// The entry binds the issuer's patient tokens to one member, so a
    /// `patient/` grant counts on an EHR's data, confined to its patient.
    pub patient: bool,
}

/// Whether `scopes` grant `permission` on a resource of `family`.
///
/// `named` is the resource a [`Resource::Path`](super::Resource::Path) names, read from the request
/// path, and `honoured` says what the caller's issuer entry lets count.
#[must_use]
pub fn granted(
    scopes: &[SmartScope],
    (family, permission): (ResourceFamily, Permission),
    named: Option<&str>,
    honoured: Honoured,
) -> bool {
    scopes.iter().any(|scope| match scope {
        SmartScope::Resource(resource) => {
            covers(resource, family, permission, named)
                && counts(resource, (family, permission), honoured)
        }
        SmartScope::Launch
        | SmartScope::LaunchContext(_)
        | SmartScope::Identity(_)
        | SmartScope::Other(_) => false,
    })
}

/// Whether every one of `covering`, the scopes that cover an operation, is
/// a `patient/` grant, so the operation is confined to the token's patient.
#[must_use]
pub fn confined(covering: &[SmartScope]) -> bool {
    !covering.is_empty()
        && covering.iter().all(|scope| {
            matches!(scope, SmartScope::Resource(resource) if resource.compartment == Compartment::Patient)
        })
}

/// Whether every one of `covering`, the scopes that cover an operation, is
/// a `system/` grant.
///
/// SMART on openEHR grants a `system/` scope "to backend applications acting
/// without a user context" (master08 §Resource Scopes).
#[must_use]
pub fn backend_only(covering: &[SmartScope]) -> bool {
    !covering.is_empty()
        && covering.iter().all(|scope| {
            matches!(scope, SmartScope::Resource(resource) if resource.compartment == Compartment::System)
        })
}

/// Whether `granted`, a token's whole grant, is a patient grant: it holds a
/// resource scope, and every resource scope it holds is a `patient/` one.
///
/// A scope that is no resource scope (`openid`, `launch/patient`) grants
/// no data, so it neither makes nor breaks a patient grant.
#[must_use]
pub fn patient_grant(granted: &[SmartScope]) -> bool {
    let resources: Vec<SmartScope> = granted
        .iter()
        .filter(|scope| matches!(scope, SmartScope::Resource(_)))
        .cloned()
        .collect();
    confined(&resources)
}

/// Whether `scope` is of `family`, holds `permission` and covers the
/// resource `named`, or every resource when the request names none.
fn covers(
    scope: &ResourceScope,
    family: ResourceFamily,
    permission: Permission,
    named: Option<&str>,
) -> bool {
    if scope.resource.family() != family || !scope.permissions.contains(permission) {
        return false;
    }
    let pattern = scope.resource.pattern();
    match named {
        Some(name) => pattern.matches(name),
        None => every(scope),
    }
}

/// Whether `scope`'s pattern covers every resource of its family.
fn every(scope: &ResourceScope) -> bool {
    matches!(scope.resource.pattern().as_str(), "*" | "**")
}

/// Whether the gateway honours `scope` for this caller on an operation of
/// `family` needing `permission`: a `patient/` grant only where the issuer
/// is bound to a member and only on an EHR's data, and a `system/aql-*`
/// grant only for a backend client the deployment lists.
fn counts(
    scope: &ResourceScope,
    (family, permission): (ResourceFamily, Permission),
    honoured: Honoured,
) -> bool {
    // NOTE: SMART on openEHR master07 §Context Selection, Federation Tier §5.2, §12.5; a bare
    // `ehrId` means nothing outside its CDR, so a patient grant counts only once its issuer is
    // bound to one member whose `ehrId` the cross-reference resolves.
    if scope.compartment == Compartment::Patient {
        return honoured.patient && on_ehr_data(family, permission);
    }
    let system_wide_aql = scope.compartment == Compartment::System
        && matches!(scope.resource, ResourceSelector::Aql(_))
        && every(scope);
    !system_wide_aql || honoured.backend
}

/// Whether an operation of `family` needing `permission` reaches a
/// patient's EHR data, the only data a `patient/` grant covers: "access is
/// restricted to data within that patient's EHR" (master08 §Resource
/// Scopes).
///
/// The `composition-` family covers an EHR's resources, and an `aql-` grant
/// covers running a query; reading or storing a query definition, and
/// every template, is no EHR's data.
fn on_ehr_data(family: ResourceFamily, permission: Permission) -> bool {
    match family {
        ResourceFamily::Composition => true,
        ResourceFamily::Aql => permission == Permission::Search,
        ResourceFamily::Template => false,
    }
}

/// Whether `client_id` is one of `backend_clients`.
#[must_use]
pub fn backend(backend_clients: &BTreeSet<String>, client_id: &str) -> bool {
    backend_clients.contains(client_id)
}

#[cfg(test)]
mod tests {
    use super::{Honoured, Permission, ResourceFamily, confined, granted, patient_grant};
    use openehr_sdt::smart_scopes::SmartScope;

    const AQL_SEARCH: (ResourceFamily, Permission) = (ResourceFamily::Aql, Permission::Search);

    #[test]
    fn an_ad_hoc_query_needs_a_wildcard_aql_search_scope_for_a_user_or_a_system() {
        for scope in ["user/aql-*.rs", "user/aql-**.cruds"] {
            assert!(
                granted(&SmartScope::parse_all(scope), AQL_SEARCH, None, NONE),
                "{scope}"
            );
        }
        for scope in [
            "patient/aql-*.s",
            "user/aql-org.example::*.s",
            "user/aql-*.r",
            "user/composition-*.s",
            "aql-*.s",
            "openid",
        ] {
            assert!(
                !granted(&SmartScope::parse_all(scope), AQL_SEARCH, None, NONE),
                "{scope}"
            );
        }
    }

    #[test]
    fn a_named_resource_is_covered_by_its_own_pattern() {
        let scopes = SmartScope::parse_all("user/aql-org.example::*.s");
        assert!(granted(
            &scopes,
            AQL_SEARCH,
            Some("org.example::vitals"),
            NONE
        ));
        assert!(!granted(
            &scopes,
            AQL_SEARCH,
            Some("org.other::vitals"),
            NONE
        ));
    }

    #[test]
    fn a_system_wide_aql_grant_counts_only_for_a_backend_client() {
        let scopes = SmartScope::parse_all("system/aql-*.s");
        assert!(!granted(&scopes, AQL_SEARCH, None, NONE));
        assert!(granted(&scopes, AQL_SEARCH, None, BACKEND));
        let named = SmartScope::parse_all("system/aql-org.example::vitals.s");
        assert!(granted(
            &named,
            AQL_SEARCH,
            Some("org.example::vitals"),
            NONE
        ));
    }

    // NOTE: SMART on openEHR master08 §Resource Scopes: a patient grant reaches "data within
    // that patient's EHR", so only with a bound issuer and never a template or a definition.
    #[test]
    fn a_patient_grant_counts_only_for_a_bound_issuer_and_only_on_ehr_data() {
        let patient = Honoured {
            patient: true,
            ..NONE
        };
        let scopes = SmartScope::parse_all("patient/aql-*.rs patient/composition-*.crud");
        assert!(!granted(&scopes, AQL_SEARCH, None, NONE), "unbound issuer");
        assert!(granted(&scopes, AQL_SEARCH, None, patient));
        let composition_read = (ResourceFamily::Composition, Permission::Read);
        assert!(granted(&scopes, composition_read, None, patient));
        let definition_read = (ResourceFamily::Aql, Permission::Read);
        assert!(!granted(&scopes, definition_read, None, patient));
        let template = SmartScope::parse_all("patient/template-*.cruds");
        let template_read = (ResourceFamily::Template, Permission::Read);
        assert!(!granted(&template, template_read, None, patient));
    }

    #[test]
    fn a_grant_of_patient_resource_scopes_alone_is_a_patient_grant() {
        assert!(patient_grant(&SmartScope::parse_all(
            "openid launch/patient patient/composition-*.r"
        )));
        assert!(!patient_grant(&SmartScope::parse_all(
            "patient/composition-*.r user/aql-*.s"
        )));
        assert!(!patient_grant(&SmartScope::parse_all(
            "openid launch/patient"
        )));
    }

    #[test]
    fn only_covering_patient_grants_confine_an_operation() {
        assert!(confined(&SmartScope::parse_all("patient/aql-*.s")));
        assert!(!confined(&SmartScope::parse_all(
            "patient/aql-*.s user/aql-*.s"
        )));
        assert!(!confined(&SmartScope::parse_all("user/aql-*.s")));
        assert!(!confined(&[]));
    }

    const NONE: Honoured = Honoured {
        backend: false,
        patient: false,
    };

    const BACKEND: Honoured = Honoured {
        backend: true,
        patient: false,
    };
}
