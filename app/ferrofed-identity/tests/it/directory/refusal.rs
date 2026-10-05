// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every refusal of the FHIR form names the resource it refuses: a connection
//! type that is no openEHR Query API code (N19, §15.2, CP-20, an operator
//! point the gateway assists), a managing organisation that is missing or does
//! not resolve (N20), an id that is missing, repeated or not unique (N19), and
//! a node its endpoints disagree on.

use ferrofed_identity::directory::error::{
    ConnectionTypeFault, FhirFormError, IdentifierFault, OperatorFault, ReferenceFault,
};
use ferrofed_identity::directory::{self, ENDPOINT_ID_SYSTEM, NODE_ID_SYSTEM, SYSTEM_ID_SYSTEM};
use ferrofed_registry::error::LoadError;
use ferrofed_registry::id::{EndpointId, NodeId, OrganisationId, SystemId};
use ferrofed_registry::snapshot::ConnectionType;
use serde_json::{Value, json};

use super::{NODE_A_PUB, NODE_A_REGION, NODE_B_PUB, ORG_A, ORG_REGION, bytes, fhir, resource};

fn load(bundle: &Value) -> Result<(), FhirFormError> {
    directory::snapshot_from_json(&bytes(bundle)).map(|_| ())
}

fn endpoint(id: &str) -> EndpointId {
    id.parse().expect("a valid endpoint id")
}

fn organisation(id: &str) -> OrganisationId {
    id.parse().expect("a valid organisation id")
}

fn with_connection_type(connection_type: &Value) -> Value {
    let mut bundle = fhir();
    resource(&mut bundle, NODE_A_PUB)["connectionType"] = connection_type.clone();
    bundle
}

fn connection_type_fault(bundle: &Value) -> ConnectionTypeFault {
    match load(bundle) {
        Err(FhirFormError::ConnectionType {
            endpoint: refused,
            fault,
        }) => {
            assert_eq!(refused, endpoint("node-a-pub"));
            fault
        }
        other => panic!("expected a connection type refusal, got {other:?}"),
    }
}

#[test]
fn hl7_fhir_rest_is_refused_in_any_system() {
    for system in [
        "http://terminology.hl7.org/CodeSystem/endpoint-connection-type",
        ConnectionType::SYSTEM,
    ] {
        let bundle = with_connection_type(&json!({"system": system, "code": "hl7-fhir-rest"}));
        assert_eq!(
            connection_type_fault(&bundle),
            ConnectionTypeFault::FhirRest
        );
    }
}

#[test]
fn an_informal_string_is_refused() {
    let bundle = with_connection_type(&json!({"code": "open-ehr-query-API"}));
    assert_eq!(
        connection_type_fault(&bundle),
        ConnectionTypeFault::NoSystem {
            code: "open-ehr-query-API".to_owned()
        }
    );
}

#[test]
fn openehr_rest_query_without_a_system_is_refused() {
    let bundle = with_connection_type(&json!({"code": "openehr-rest-query"}));
    assert_eq!(
        connection_type_fault(&bundle),
        ConnectionTypeFault::NoSystem {
            code: "openehr-rest-query".to_owned()
        }
    );
}

#[test]
fn a_code_in_an_unbound_system_is_refused() {
    let bundle = with_connection_type(
        &json!({"system": "https://example.org/connection-type", "code": "openehr-rest-query"}),
    );
    assert_eq!(
        connection_type_fault(&bundle),
        ConnectionTypeFault::Unbound {
            system: "https://example.org/connection-type".to_owned(),
            code: "openehr-rest-query".to_owned()
        }
    );
}

#[test]
fn a_code_the_bound_system_does_not_define_is_refused() {
    let bundle =
        with_connection_type(&json!({"system": ConnectionType::SYSTEM, "code": "openehr-rest"}));
    assert_eq!(
        connection_type_fault(&bundle),
        ConnectionTypeFault::Unbound {
            system: ConnectionType::SYSTEM.to_owned(),
            code: "openehr-rest".to_owned()
        }
    );
}

#[test]
fn a_connection_type_without_a_code_is_refused() {
    let bundle = with_connection_type(&json!({"system": ConnectionType::SYSTEM}));
    assert_eq!(connection_type_fault(&bundle), ConnectionTypeFault::NoCode);
}

#[test]
fn the_connection_type_refusal_names_the_endpoint() {
    let bundle = with_connection_type(&json!({"code": "hl7-fhir-rest"}));
    let error = load(&bundle).expect_err("hl7-fhir-rest is refused");
    assert!(error.to_string().contains("node-a-pub"), "{error}");
}

fn managing_fault(bundle: &Value) -> ReferenceFault {
    match load(bundle) {
        Err(FhirFormError::ManagingOrganisation {
            endpoint: refused,
            fault,
        }) => {
            assert_eq!(refused, endpoint("node-a-pub"));
            fault
        }
        other => panic!("expected a managing organisation refusal, got {other:?}"),
    }
}

#[test]
fn an_endpoint_without_a_managing_organisation_is_refused() {
    let mut bundle = fhir();
    if let Some(endpoint) = resource(&mut bundle, NODE_A_PUB).as_object_mut() {
        endpoint.remove("managingOrganization");
    }
    assert_eq!(managing_fault(&bundle), ReferenceFault::Missing);
}

#[test]
fn a_managing_organisation_outside_the_bundle_is_refused() {
    let mut bundle = fhir();
    resource(&mut bundle, NODE_A_PUB)["managingOrganization"] =
        json!({"reference": "Organization/org-elsewhere"});
    assert_eq!(
        managing_fault(&bundle),
        ReferenceFault::Outside("Organization/org-elsewhere".to_owned())
    );
}

#[test]
fn a_managing_organisation_by_identifier_alone_is_refused() {
    let mut bundle = fhir();
    resource(&mut bundle, NODE_A_PUB)["managingOrganization"] = json!({
        "identifier": {"system": "https://ferrofed.eu/fhir/sid/organisation-id", "value": "org-a"}
    });
    assert_eq!(managing_fault(&bundle), ReferenceFault::NotLiteral);
}

#[test]
fn an_endpoint_two_organisations_list_is_refused() {
    let mut bundle = fhir();
    resource(&mut bundle, ORG_REGION)["endpoint"] = json!([
        {"reference": "Endpoint/node-b-pub"},
        {"reference": "Endpoint/node-a-pub"},
    ]);
    assert_eq!(
        match load(&bundle) {
            Err(FhirFormError::Operator { endpoint, fault }) => Some((endpoint, fault)),
            _ => None,
        },
        Some((
            endpoint("node-a-pub"),
            OperatorFault::Several {
                first: organisation("org-a"),
                second: organisation("org-region"),
            }
        ))
    );
}

#[test]
fn an_endpoint_listed_by_no_organisation_is_refused() {
    let mut unlisted = fhir();
    if let Some(org) = resource(&mut unlisted, ORG_REGION).as_object_mut() {
        org.remove("endpoint");
    }
    assert_eq!(
        match load(&unlisted) {
            Err(FhirFormError::Operator { endpoint, fault }) => Some((endpoint, fault)),
            _ => None,
        },
        Some((endpoint("node-b-pub"), OperatorFault::Unlisted))
    );
}

#[test]
fn a_listing_of_an_endpoint_outside_the_bundle_is_refused() {
    let mut bundle = fhir();
    resource(&mut bundle, ORG_REGION)["endpoint"] = json!([
        {"reference": "Endpoint/node-b-pub"},
        {"reference": "Endpoint/node-z"},
    ]);
    assert_eq!(
        match load(&bundle) {
            Err(FhirFormError::OrganisationEndpoint {
                organisation,
                fault,
            }) => Some((organisation, fault)),
            _ => None,
        },
        Some((
            organisation("org-region"),
            ReferenceFault::Outside("Endpoint/node-z".to_owned())
        ))
    );
}

/// A refused listing reads as one of an endpoint, never as a reference to an
/// organisation.
#[test]
fn a_refused_listing_names_the_reference_and_no_organization_type() {
    let message = ReferenceFault::Outside("Endpoint/node-z".to_owned()).to_string();
    assert_eq!(
        "the reference \"Endpoint/node-z\" names no resource of the Bundle",
        message
    );
}

#[test]
fn the_same_organisation_listing_an_endpoint_twice_is_one_operator() {
    let mut bundle = fhir();
    resource(&mut bundle, ORG_REGION)["endpoint"] = json!([
        {"reference": "Endpoint/node-b-pub"},
        {"reference": "Endpoint/node-b-pub"},
    ]);
    assert!(load(&bundle).is_ok());
}

fn identifiers(bundle: &mut Value, index: usize) -> &mut Value {
    &mut resource(bundle, index)["identifier"]
}

#[test]
fn an_endpoint_without_an_endpoint_id_is_refused() {
    let mut bundle = fhir();
    identifiers(&mut bundle, NODE_A_REGION)[0]["system"] = json!("https://example.org/other");
    let refused = load(&bundle);
    let Err(FhirFormError::EndpointId { endpoint, fault }) = &refused else {
        panic!("expected an endpoint id refusal, got {refused:?}");
    };
    assert_eq!(fault, &IdentifierFault::Missing);
    assert_eq!(
        (endpoint.index(), endpoint.logical_id()),
        (NODE_A_REGION, Some("node-a-region"))
    );
    let message = refused.map_err(|error| error.to_string()).err();
    assert!(
        message
            .as_deref()
            .is_some_and(|message| message.contains("Bundle.entry[3] (id \"node-a-region\")")),
        "{message:?}"
    );
}

#[test]
fn an_endpoint_with_two_endpoint_ids_is_refused() {
    let mut bundle = fhir();
    if let Some(list) = identifiers(&mut bundle, NODE_B_PUB).as_array_mut() {
        list.push(json!({"system": ENDPOINT_ID_SYSTEM, "value": "node-b-other"}));
    }
    assert!(matches!(
        load(&bundle),
        Err(FhirFormError::EndpointId {
            fault: IdentifierFault::Repeated,
            ..
        })
    ));
}

#[test]
fn an_endpoint_id_without_a_value_is_refused() {
    let mut bundle = fhir();
    identifiers(&mut bundle, NODE_B_PUB)[0] = json!({"system": ENDPOINT_ID_SYSTEM});
    assert!(matches!(
        load(&bundle),
        Err(FhirFormError::EndpointId {
            fault: IdentifierFault::NoValue,
            ..
        })
    ));
}

#[test]
fn a_malformed_endpoint_id_is_refused() {
    let mut bundle = fhir();
    identifiers(&mut bundle, NODE_B_PUB)[0]["value"] = json!("node b");
    assert!(matches!(
        load(&bundle),
        Err(FhirFormError::EndpointId {
            fault: IdentifierFault::Malformed(_),
            ..
        })
    ));
}

#[test]
fn two_endpoints_with_one_endpoint_id_are_refused() {
    let mut bundle = fhir();
    identifiers(&mut bundle, NODE_A_REGION)[0]["value"] = json!("node-a-pub");
    assert!(matches!(
        load(&bundle),
        Err(FhirFormError::Registry(LoadError::DuplicateEndpoint(id))) if id == endpoint("node-a-pub")
    ));
}

#[test]
fn two_organisations_with_one_id_are_refused() {
    let mut bundle = fhir();
    identifiers(&mut bundle, ORG_REGION)[0]["value"] = json!("org-a");
    assert!(matches!(
        load(&bundle),
        Err(FhirFormError::Registry(LoadError::DuplicateOrganisation(id))) if id == organisation("org-a")
    ));
}

#[test]
fn an_organisation_without_an_id_is_refused() {
    let mut bundle = fhir();
    if let Some(org) = resource(&mut bundle, ORG_REGION).as_object_mut() {
        org.remove("identifier");
    }
    assert!(matches!(
        load(&bundle),
        Err(FhirFormError::OrganisationId {
            organisation,
            fault: IdentifierFault::Missing,
        }) if organisation.index() == ORG_REGION
    ));
}

#[test]
fn an_inactive_organisation_is_refused() {
    let mut bundle = fhir();
    resource(&mut bundle, ORG_REGION)["active"] = json!(false);
    assert!(matches!(
        load(&bundle),
        Err(FhirFormError::OrganisationInactive(id)) if id == organisation("org-region")
    ));
}

#[test]
fn an_endpoint_without_a_node_id_or_a_system_id_is_refused() {
    for (system, index) in [(NODE_ID_SYSTEM, 1), (SYSTEM_ID_SYSTEM, 2)] {
        let mut bundle = fhir();
        identifiers(&mut bundle, NODE_B_PUB)[index]["system"] = json!("https://example.org/x");
        let refused = load(&bundle);
        let matched = match &refused {
            Err(FhirFormError::NodeId { endpoint: id, .. }) => {
                system == NODE_ID_SYSTEM && *id == endpoint("node-b-pub")
            }
            Err(FhirFormError::SystemId { endpoint: id, .. }) => {
                system == SYSTEM_ID_SYSTEM && *id == endpoint("node-b-pub")
            }
            _ => false,
        };
        assert!(matched, "{system}: {refused:?}");
    }
}

#[test]
fn the_endpoints_of_one_node_disagreeing_on_its_system_id_are_refused() {
    let mut bundle = fhir();
    identifiers(&mut bundle, NODE_A_REGION)[2]["value"] = json!("cdr-a2.example.org");
    assert!(matches!(
        load(&bundle),
        Err(FhirFormError::NodeSystemId { node, first, second })
            if node == "node-a".parse::<NodeId>().expect("a node id")
                && first == "cdr-a.example.org".parse::<SystemId>().expect("a system id")
                && second == "cdr-a2.example.org".parse::<SystemId>().expect("a system id")
    ));
}

#[test]
fn the_endpoints_of_one_node_disagreeing_on_its_operator_are_refused() {
    let mut bundle = fhir();
    resource(&mut bundle, ORG_A)["endpoint"] = json!([{"reference": "Endpoint/node-a-pub"}]);
    resource(&mut bundle, ORG_REGION)["endpoint"] = json!([
        {"reference": "Endpoint/node-a-region"},
        {"reference": "Endpoint/node-b-pub"},
    ]);
    assert!(matches!(
        load(&bundle),
        Err(FhirFormError::NodeOperator { node, first, second })
            if node == "node-a".parse::<NodeId>().expect("a node id")
                && first == organisation("org-a")
                && second == organisation("org-region")
    ));
}

#[test]
fn two_nodes_with_one_system_id_are_refused() {
    let mut bundle = fhir();
    identifiers(&mut bundle, NODE_B_PUB)[2]["value"] = json!("CDR-A.example.org");
    assert!(matches!(
        load(&bundle),
        Err(FhirFormError::Registry(LoadError::DuplicateSystemId { .. }))
    ));
}

#[test]
fn a_status_other_than_active_or_suspended_is_refused() {
    for status in ["off", "error", "test", "entered-in-error"] {
        let mut bundle = fhir();
        resource(&mut bundle, NODE_B_PUB)["status"] = json!(status);
        assert!(
            matches!(
                load(&bundle),
                Err(FhirFormError::Status { endpoint: id, found: Some(found) })
                    if id == endpoint("node-b-pub") && found == status
            ),
            "{status}"
        );
    }
}

#[test]
fn an_unusable_address_is_refused() {
    let mut bundle = fhir();
    resource(&mut bundle, NODE_B_PUB)["address"] = json!("ftp://cdr-b.example.org/openehr");
    assert!(matches!(
        load(&bundle),
        Err(FhirFormError::Registry(LoadError::EndpointUrl { endpoint: id, .. })) if id == endpoint("node-b-pub")
    ));
}

#[test]
fn a_creating_system_id_that_is_a_members_own_is_refused() {
    let mut bundle = fhir();
    identifiers(&mut bundle, NODE_A_PUB)[3]["value"] = json!("2.999.20.1");
    assert!(matches!(
        load(&bundle),
        Err(FhirFormError::Registry(
            LoadError::CreatingSystemIdOfNode { .. }
        ))
    ));
}

#[test]
fn a_malformed_creating_system_id_is_refused() {
    let mut bundle = fhir();
    identifiers(&mut bundle, NODE_A_PUB)[3]["value"] = json!("not a uid");
    assert!(matches!(
        load(&bundle),
        Err(FhirFormError::CreatingSystemId {
            endpoint: id,
            fault: IdentifierFault::Malformed(_),
        }) if id == endpoint("node-a-pub")
    ));
}

#[test]
fn a_document_that_is_no_bundle_of_the_two_resources_is_refused() {
    let mut bundle = fhir();
    bundle["entry"][ORG_A]["resource"] = json!({"resourceType": "Basic", "code": {"text": "x"}});
    assert!(matches!(load(&bundle), Err(FhirFormError::Directory(_))));
    assert!(matches!(
        directory::snapshot_from_json(b"[[organisation]]"),
        Err(FhirFormError::Directory(_))
    ));
}

#[test]
fn a_document_with_no_endpoint_admits_no_node() {
    let bundle = json!({
        "resourceType": "Bundle",
        "type": "collection",
        "entry": [{
            "fullUrl": "https://registry.example.org/fhir/Organization/org-a",
            "resource": {
                "resourceType": "Organization",
                "identifier": [{"system": "https://ferrofed.eu/fhir/sid/organisation-id", "value": "org-a"}]
            }
        }]
    });
    assert!(matches!(
        load(&bundle),
        Err(FhirFormError::Registry(LoadError::NoNode))
    ));
}

#[test]
fn a_missing_file_is_refused_with_its_path() {
    let path = std::env::temp_dir().join("ferrofed-identity-directory-absent.json");
    assert!(matches!(
        directory::read(&path),
        Err(FhirFormError::Read { path: refused, .. }) if refused == path
    ));
}
