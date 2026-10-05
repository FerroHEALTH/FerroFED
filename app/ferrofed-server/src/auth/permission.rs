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

use std::collections::BTreeSet;

use openehr_its::rest::routes::RouteMatch;
use openehr_sdt::smart_scopes::{
    Compartment, Permission, ResourceFamily, ResourceScope, ResourceSelector, SmartScope,
};

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

/// A requirement for a scope of `family` granting `permission` on
/// `resource`.
const fn scope(family: ResourceFamily, permission: Permission, resource: Resource) -> Requirement {
    Requirement::Scope {
        family,
        permission,
        resource,
    }
}

/// A requirement on an EHR resource that is not a COMPOSITION: the
/// `composition-` family, over every template, with `permission`.
const fn ehr(permission: Permission) -> Requirement {
    scope(ResourceFamily::Composition, permission, Resource::Unnamed)
}

/// The ad hoc query's resource and the stored query's.
const QUERY: Resource = Resource::Path("qualified_query_name");

/// A template named in the path.
const TEMPLATE: Resource = Resource::Path("template_id");

/// What every ITS-REST operation requires, by API group and operation id.
///
/// The `openehr-its` route table names every operation the gateway can be
/// asked for; a test holds this table to it, one entry per operation.
pub const TABLE: &[(&str, &str, Requirement)] = &[
    ("admin", "admin_ehr_delete", Requirement::Refused),
    ("admin", "admin_ehr_delete_all", Requirement::Refused),
    (
        "definition",
        "definition_template_adl1.4_list",
        scope(
            ResourceFamily::Template,
            Permission::Read,
            Resource::Unnamed,
        ),
    ),
    (
        "definition",
        "definition_template_adl1.4_upload",
        scope(
            ResourceFamily::Template,
            Permission::Create,
            Resource::Unnamed,
        ),
    ),
    (
        "definition",
        "definition_template_adl1.4_get",
        scope(ResourceFamily::Template, Permission::Read, TEMPLATE),
    ),
    (
        "definition",
        "definition_template_adl1.4_example_get",
        scope(ResourceFamily::Template, Permission::Read, TEMPLATE),
    ),
    (
        "definition",
        "definition_template_adl2_list",
        scope(
            ResourceFamily::Template,
            Permission::Read,
            Resource::Unnamed,
        ),
    ),
    (
        "definition",
        "definition_template_adl2_upload",
        scope(
            ResourceFamily::Template,
            Permission::Create,
            Resource::Unnamed,
        ),
    ),
    (
        "definition",
        "definition_template_adl2_get",
        scope(ResourceFamily::Template, Permission::Read, TEMPLATE),
    ),
    (
        "definition",
        "definition_template_adl2_example_get",
        scope(ResourceFamily::Template, Permission::Read, TEMPLATE),
    ),
    (
        "definition",
        "definition_template_adl2_version_get",
        scope(ResourceFamily::Template, Permission::Read, TEMPLATE),
    ),
    (
        "definition",
        "definition_query_list",
        scope(ResourceFamily::Aql, Permission::Read, QUERY),
    ),
    (
        "definition",
        "definition_query_store.yaml",
        scope(ResourceFamily::Aql, Permission::Create, QUERY),
    ),
    (
        "definition",
        "definition_query_version_get",
        scope(ResourceFamily::Aql, Permission::Read, QUERY),
    ),
    (
        "definition",
        "definition_query_version_store.yaml",
        scope(ResourceFamily::Aql, Permission::Create, QUERY),
    ),
    ("demographic", "agent_create", Requirement::Demographic),
    ("demographic", "agent_get", Requirement::Demographic),
    ("demographic", "agent_update", Requirement::Demographic),
    ("demographic", "agent_delete", Requirement::Demographic),
    ("demographic", "group_create", Requirement::Demographic),
    ("demographic", "group_get", Requirement::Demographic),
    ("demographic", "group_update", Requirement::Demographic),
    ("demographic", "group_delete", Requirement::Demographic),
    (
        "demographic",
        "organisation_create",
        Requirement::Demographic,
    ),
    ("demographic", "organisation_get", Requirement::Demographic),
    (
        "demographic",
        "organisation_update",
        Requirement::Demographic,
    ),
    (
        "demographic",
        "organisation_delete",
        Requirement::Demographic,
    ),
    ("demographic", "person_create", Requirement::Demographic),
    ("demographic", "person_get", Requirement::Demographic),
    ("demographic", "person_update", Requirement::Demographic),
    ("demographic", "person_delete", Requirement::Demographic),
    ("demographic", "role_create", Requirement::Demographic),
    ("demographic", "role_get", Requirement::Demographic),
    ("demographic", "role_update", Requirement::Demographic),
    ("demographic", "role_delete", Requirement::Demographic),
    (
        "demographic",
        "versioned_party_get",
        Requirement::Demographic,
    ),
    (
        "demographic",
        "versioned_party_revision_history",
        Requirement::Demographic,
    ),
    (
        "demographic",
        "versioned_party_version_get_at_time",
        Requirement::Demographic,
    ),
    (
        "demographic",
        "versioned_party_version_get_by_id",
        Requirement::Demographic,
    ),
    (
        "demographic",
        "contribution_create",
        Requirement::Demographic,
    ),
    ("demographic", "contribution_get", Requirement::Demographic),
    (
        "demographic",
        "demographic_tags_get",
        Requirement::Demographic,
    ),
    ("demographic", "agent_tags_get", Requirement::Demographic),
    ("demographic", "agent_tags_update", Requirement::Demographic),
    ("demographic", "agent_tags_delete", Requirement::Demographic),
    ("demographic", "group_tags_get", Requirement::Demographic),
    ("demographic", "group_tags_update", Requirement::Demographic),
    ("demographic", "group_tags_delete", Requirement::Demographic),
    (
        "demographic",
        "organisation_tags_get",
        Requirement::Demographic,
    ),
    (
        "demographic",
        "organisation_tags_update",
        Requirement::Demographic,
    ),
    (
        "demographic",
        "organisation_tags_delete",
        Requirement::Demographic,
    ),
    ("demographic", "person_tags_get", Requirement::Demographic),
    (
        "demographic",
        "person_tags_update",
        Requirement::Demographic,
    ),
    (
        "demographic",
        "person_tags_delete",
        Requirement::Demographic,
    ),
    ("demographic", "role_tags_get", Requirement::Demographic),
    ("demographic", "role_tags_update", Requirement::Demographic),
    ("demographic", "role_tags_delete", Requirement::Demographic),
    ("ehr", "ehr_get_by_subject", ehr(Permission::Read)),
    ("ehr", "ehr_create", ehr(Permission::Create)),
    ("ehr", "ehr_get_by_id", ehr(Permission::Read)),
    ("ehr", "ehr_create_with_id", ehr(Permission::Create)),
    ("ehr", "ehr_status_get_by_version_id", ehr(Permission::Read)),
    ("ehr", "ehr_status_get_at_time", ehr(Permission::Read)),
    ("ehr", "ehr_status_update", ehr(Permission::Update)),
    ("ehr", "versioned_ehr_status_get", ehr(Permission::Read)),
    (
        "ehr",
        "versioned_ehr_status_revision_history",
        ehr(Permission::Read),
    ),
    (
        "ehr",
        "versioned_ehr_status_version_get_at_time",
        ehr(Permission::Read),
    ),
    (
        "ehr",
        "versioned_ehr_status_version_get_by_id",
        ehr(Permission::Read),
    ),
    (
        "ehr",
        "composition_create",
        scope(
            ResourceFamily::Composition,
            Permission::Create,
            Resource::Unnamed,
        ),
    ),
    (
        "ehr",
        "composition_get",
        scope(
            ResourceFamily::Composition,
            Permission::Read,
            Resource::Unnamed,
        ),
    ),
    (
        "ehr",
        "composition_update",
        scope(
            ResourceFamily::Composition,
            Permission::Update,
            Resource::Unnamed,
        ),
    ),
    (
        "ehr",
        "composition_delete",
        scope(
            ResourceFamily::Composition,
            Permission::Delete,
            Resource::Unnamed,
        ),
    ),
    (
        "ehr",
        "versioned_composition_get",
        scope(
            ResourceFamily::Composition,
            Permission::Read,
            Resource::Unnamed,
        ),
    ),
    (
        "ehr",
        "versioned_composition_revision_history",
        scope(
            ResourceFamily::Composition,
            Permission::Read,
            Resource::Unnamed,
        ),
    ),
    (
        "ehr",
        "versioned_composition_version_get_at_time",
        scope(
            ResourceFamily::Composition,
            Permission::Read,
            Resource::Unnamed,
        ),
    ),
    (
        "ehr",
        "versioned_composition_version_get_by_id",
        scope(
            ResourceFamily::Composition,
            Permission::Read,
            Resource::Unnamed,
        ),
    ),
    ("ehr", "directory_get_at_time", ehr(Permission::Read)),
    ("ehr", "directory_update", ehr(Permission::Update)),
    ("ehr", "directory_create", ehr(Permission::Create)),
    ("ehr", "directory_delete", ehr(Permission::Delete)),
    ("ehr", "directory_get_by_version_id", ehr(Permission::Read)),
    ("ehr", "contribution_create", ehr(Permission::Create)),
    ("ehr", "contribution_get", ehr(Permission::Read)),
    ("ehr", "ehr_tags_get", ehr(Permission::Read)),
    (
        "ehr",
        "composition_tags_get",
        scope(
            ResourceFamily::Composition,
            Permission::Read,
            Resource::Unnamed,
        ),
    ),
    (
        "ehr",
        "composition_tags_update",
        scope(
            ResourceFamily::Composition,
            Permission::Update,
            Resource::Unnamed,
        ),
    ),
    (
        "ehr",
        "composition_tags_delete",
        scope(
            ResourceFamily::Composition,
            Permission::Delete,
            Resource::Unnamed,
        ),
    ),
    ("ehr", "ehr_status_tags_get", ehr(Permission::Read)),
    ("ehr", "ehr_status_tags_update", ehr(Permission::Update)),
    ("ehr", "ehr_status_tags_delete", ehr(Permission::Delete)),
    (
        "query",
        "query_execute_adhoc_query",
        scope(ResourceFamily::Aql, Permission::Search, Resource::Unnamed),
    ),
    (
        "query",
        "query_execute_adhoc_query_body",
        scope(ResourceFamily::Aql, Permission::Search, Resource::Unnamed),
    ),
    (
        "query",
        "query_execute_stored_query",
        scope(ResourceFamily::Aql, Permission::Search, QUERY),
    ),
    (
        "query",
        "query_execute_stored_query_body",
        scope(ResourceFamily::Aql, Permission::Search, QUERY),
    ),
    (
        "query",
        "query_execute_stored_query_version",
        scope(ResourceFamily::Aql, Permission::Search, QUERY),
    ),
    (
        "query",
        "query_execute_stored_query_version_body",
        scope(ResourceFamily::Aql, Permission::Search, QUERY),
    ),
    ("system", "options", Requirement::Caller),
];

/// What the operation `matched` names requires, or `None` for an operation
/// the table does not list.
#[must_use]
pub fn of(matched: &RouteMatch) -> Option<Requirement> {
    TABLE
        .iter()
        .find(|(group, operation, _)| *group == matched.group && *operation == matched.operation_id)
        .map(|&(_, _, requirement)| requirement)
}

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
/// `named` is the resource a [`Resource::Path`] names, read from the request
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
    use std::collections::BTreeSet;

    use super::{Honoured, Permission, ResourceFamily, TABLE, confined, granted, patient_grant};
    use openehr_its::rest::generated::{admin, definition, demographic, ehr, query, system};
    use openehr_sdt::smart_scopes::SmartScope;

    // The router serves every ITS-REST operation through the `openehr-its`
    // route table, so the table lists each one, once, and nothing else.
    #[test]
    fn the_table_lists_every_operation_of_the_route_table_once() {
        let groups = [
            ("admin", admin::ROUTES),
            ("definition", definition::ROUTES),
            ("demographic", demographic::ROUTES),
            ("ehr", ehr::ROUTES),
            ("query", query::ROUTES),
            ("system", system::ROUTES),
        ];
        let served: BTreeSet<(&str, &str)> = groups
            .iter()
            .flat_map(|(group, routes)| {
                routes
                    .iter()
                    .map(move |(_, _, operation)| (*group, *operation))
            })
            .collect();
        let listed: Vec<(&str, &str)> = TABLE
            .iter()
            .map(|(group, operation, _)| (*group, *operation))
            .collect();
        let distinct: BTreeSet<(&str, &str)> = listed.iter().copied().collect();
        assert_eq!(listed.len(), distinct.len(), "an operation is listed twice");
        assert_eq!(served, distinct);
    }

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
