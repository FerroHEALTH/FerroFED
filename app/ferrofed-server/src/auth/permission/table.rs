// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The requirement of every ITS-REST operation the façade can be asked for,
//! one entry per operation of the `openehr-its` route table (§13.1, N25;
//! ITS-REST SMART on openEHR, master08 §Resource Scopes).

use openehr_sdt::smart_scopes::{Permission, ResourceFamily};

use super::{Requirement, Resource};

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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::TABLE;
    use openehr_its::rest::generated::{admin, definition, demographic, ehr, query, system};

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
}
