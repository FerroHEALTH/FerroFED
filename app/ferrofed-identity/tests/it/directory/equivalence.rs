// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A document in FHIR form loads into the same snapshot as the same registry
//! in native form, so it routes identically (N19, N20, N21, §15.1).

use std::error::Error;

use ferrofed_identity::directory;
use ferrofed_identity::directory::error::{FhirFormError, ReferenceFault};
use ferrofed_registry::creating_system::CreatingSystemRoute;
use ferrofed_registry::id::{EndpointId, NodeId, OrganisationId, SystemId};
use ferrofed_registry::snapshot::{Endpoint, EndpointStatus, RegistrySnapshot};
use serde_json::json;

use super::{NATIVE, NODE_A_PUB, ORG_A, bytes, fhir, resource};

#[test]
fn the_fhir_form_loads_the_snapshot_the_native_form_loads() -> Result<(), Box<dyn Error>> {
    let native = RegistrySnapshot::from_toml_str(NATIVE)?;
    let fhir = directory::snapshot_from_json(&bytes(&fhir()))?;
    assert_eq!(native, fhir);
    Ok(())
}

#[test]
fn the_fhir_form_routes_as_the_native_form_routes() -> Result<(), Box<dyn Error>> {
    let native = RegistrySnapshot::from_toml_str(NATIVE)?;
    let fhir = directory::snapshot_from_json(&bytes(&fhir()))?;
    for registry in [&native, &fhir] {
        let member: SystemId = "cdr-a.example.org".parse()?;
        assert_eq!(
            registry.registered_route(&member),
            Some(CreatingSystemRoute::Member {
                node: "node-a".parse()?
            })
        );
        let legacy: SystemId = "LEGACY-A.example.org".parse()?;
        assert_eq!(
            registry.registered_route(&legacy),
            Some(CreatingSystemRoute::Registered {
                node: "node-a".parse()?,
                endpoint: "node-a-pub".parse()?,
            })
        );
        let region: OrganisationId = "org-region".parse()?;
        let managed: Vec<&EndpointId> = registry
            .endpoints_managed_by(&region)
            .map(Endpoint::id)
            .collect();
        assert_eq!(
            managed,
            vec![
                &"node-a-region".parse::<EndpointId>()?,
                &"node-b-pub".parse::<EndpointId>()?,
            ]
        );
        let node_a: NodeId = "node-a".parse()?;
        let node = registry.node(&node_a).ok_or("node-a is a member")?;
        assert_eq!(node.organisation(), &"org-a".parse::<OrganisationId>()?);
        let suspended: EndpointId = "node-a-region".parse()?;
        let endpoint = registry.endpoint(&suspended).ok_or("an endpoint")?;
        assert_eq!(endpoint.status(), EndpointStatus::Suspended);
        assert_eq!(
            endpoint.url().as_str(),
            "https://internal.cdr-a.example.org/openehr"
        );
    }
    Ok(())
}

#[test]
fn a_searchset_with_absolute_references_loads_the_same_snapshot() -> Result<(), Box<dyn Error>> {
    let mut bundle = fhir();
    bundle["type"] = json!("searchset");
    let organisation_url = "urn:uuid:5f0c2a3e-8d71-4b6a-9e2f-1c3d4b5a6e70";
    bundle["entry"][ORG_A]["fullUrl"] = json!(organisation_url);
    let endpoint_url = "urn:uuid:7a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d";
    bundle["entry"][NODE_A_PUB]["fullUrl"] = json!(endpoint_url);
    resource(&mut bundle, ORG_A)["endpoint"][0]["reference"] = json!(endpoint_url);
    // A relative reference has no meaning from a urn:uuid entry (FHIR R4
    // §2.36.4.1), so the organisation's other listing is absolute too.
    resource(&mut bundle, ORG_A)["endpoint"][1]["reference"] =
        json!("https://registry.example.org/fhir/Endpoint/node-a-region");
    resource(&mut bundle, NODE_A_PUB)["managingOrganization"]["reference"] =
        json!(organisation_url);
    let native = RegistrySnapshot::from_toml_str(NATIVE)?;
    assert_eq!(native, directory::snapshot_from_json(&bytes(&bundle))?);
    Ok(())
}

#[test]
fn a_relative_reference_from_a_urn_entry_resolves_nothing() {
    let mut bundle = fhir();
    bundle["entry"][ORG_A]["fullUrl"] = json!("urn:uuid:5f0c2a3e-8d71-4b6a-9e2f-1c3d4b5a6e70");
    assert!(matches!(
        directory::snapshot_from_json(&bytes(&bundle)),
        Err(FhirFormError::OrganisationEndpoint {
            fault: ReferenceFault::Outside(reference),
            ..
        }) if reference == "Endpoint/node-a-pub"
    ));
}

#[test]
fn a_document_is_read_from_a_file() -> Result<(), Box<dyn Error>> {
    let path = std::env::temp_dir().join(format!(
        "ferrofed-identity-directory-{}.json",
        std::process::id()
    ));
    std::fs::write(&path, bytes(&fhir()))?;
    let loaded = directory::read(&path);
    std::fs::remove_file(&path)?;
    assert_eq!(RegistrySnapshot::from_toml_str(NATIVE)?, loaded?);
    Ok(())
}
