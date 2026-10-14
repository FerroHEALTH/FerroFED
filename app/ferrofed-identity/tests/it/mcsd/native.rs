// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A member's own exchange-format endpoint in FHIR form and in an mCSD
//! directory: an `hl7-fhir-rest` `Endpoint` that names its categories as
//! `payloadType` codings, its profiles and the identifier system of the
//! member's `ehr_id` in extensions loads as the native endpoint the native
//! form declares, apart from every Query API endpoint (N19, §7a.2, §8), and
//! is refused without them as the native form refuses it (N33).

use std::error::Error;
use std::time::Duration;

use ferrofed_identity::fhir::{Authentication, Tls};
use ferrofed_identity::ihe::mcsd::error::{FhirFormError, IdentifierFault};
use ferrofed_identity::ihe::mcsd::source::{DirectoryConfig, DirectorySource, Refreshed};
use ferrofed_identity::ihe::mcsd::{
    self, CATEGORY_SYSTEM, CONSENT_REFUSAL_CODE_EXTENSION, EHR_ID_SYSTEM_EXTENSION,
    ENDPOINT_ID_SYSTEM, NODE_ID_SYSTEM, PROFILE_EXTENSION, SYSTEM_ID_SYSTEM,
};
use ferrofed_registry::error::LoadError;
use ferrofed_registry::id::{EndpointId, NodeId, OrganisationId};
use ferrofed_registry::native::NativeEndpoint;
use ferrofed_registry::secret::SecretUrl;
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_testkit::mcsd::{self as harness, HarnessDirectory, Member, Native};
use serde_json::{Value, json};

use super::{NATIVE, NODE_A_PUB, ORG_A, bytes, fhir, resource};

type TestResult = Result<(), Box<dyn Error>>;

/// The patient summary category's code.
const SUMMARIES: &str = "Patient-Summaries";

/// The EPS composition profile.
const EPS: &str = "http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps";

/// A synthetic identifier system under the example arc.
const EHR_ID_SYSTEM: &str = "urn:oid:2.999.4.1";

/// The FHIR form of a native endpoint `node-a-eps` of `node-a`, serving
/// patient summaries.
fn native_endpoint() -> Value {
    json!({
        "fullUrl": "https://registry.example.org/fhir/Endpoint/node-a-eps",
        "resource": {
            "resourceType": "Endpoint",
            "id": "node-a-eps",
            "extension": [
                {"url": PROFILE_EXTENSION, "valueCanonical": EPS},
                {"url": EHR_ID_SYSTEM_EXTENSION, "valueUri": EHR_ID_SYSTEM},
            ],
            "identifier": [
                {"system": ENDPOINT_ID_SYSTEM, "value": "node-a-eps"},
                {"system": NODE_ID_SYSTEM, "value": "node-a"},
                {"system": SYSTEM_ID_SYSTEM, "value": "cdr-a.example.org"},
            ],
            "status": "active",
            "connectionType": {"system": mcsd::HL7_CONNECTION_TYPE_SYSTEM, "code": "hl7-fhir-rest"},
            "managingOrganization": {"reference": "Organization/org-a"},
            "payloadType": [{"coding": [{"system": CATEGORY_SYSTEM, "code": SUMMARIES}]}],
            "payloadMimeType": ["application/fhir+json"],
            "address": "https://cdr-a.example.org/fhir",
        }
    })
}

/// The federation of [`fhir`] with `endpoint` appended, listed by `org-a`.
fn with(endpoint: Value) -> Value {
    let mut bundle = fhir();
    let id = endpoint["resource"]["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    if let Some(listed) = resource(&mut bundle, ORG_A)["endpoint"].as_array_mut() {
        listed.push(json!({"reference": format!("Endpoint/{id}")}));
    }
    if let Some(entries) = bundle["entry"].as_array_mut() {
        entries.push(endpoint);
    }
    bundle
}

/// The appended native endpoint's resource, for a test that breaks it.
fn native_resource(bundle: &mut Value) -> &mut Value {
    let last = bundle["entry"].as_array().map_or(0, Vec::len) - 1;
    resource(bundle, last)
}

fn load(bundle: &Value) -> Result<RegistrySnapshot, FhirFormError> {
    mcsd::snapshot_from_json(&bytes(bundle))
}

fn registry_refusal(bundle: &Value) -> LoadError {
    match load(bundle) {
        Err(FhirFormError::Registry(error)) => error,
        other => panic!("expected a membership refusal, got {other:?}"),
    }
}

fn endpoint(id: &str) -> EndpointId {
    id.parse().expect("a valid endpoint id")
}

#[test]
fn a_native_endpoint_in_fhir_form_loads_apart_from_every_query_endpoint() -> TestResult {
    let snapshot = load(&with(native_endpoint()))?;
    let id = endpoint("node-a-eps");
    assert!(
        snapshot.endpoint(&id).is_none(),
        "N19, §8: a native endpoint is no Query API endpoint"
    );
    assert_eq!(
        snapshot
            .endpoints()
            .map(|endpoint| endpoint.id().as_str())
            .collect::<Vec<_>>(),
        ["node-a-pub", "node-a-region", "node-b-pub"],
        "§7a.2: the endpoints a directive may name"
    );
    let held = snapshot.native_endpoint(&id).ok_or("the native endpoint")?;
    assert_eq!(held.ehr_id_system(), EHR_ID_SYSTEM);
    assert_eq!(held.profiles(), [EPS]);
    assert_eq!(held.url().as_str(), "https://cdr-a.example.org/fhir");
    let node: NodeId = "node-a".parse()?;
    assert_eq!(
        snapshot
            .native_for(&node, SUMMARIES)
            .map(NativeEndpoint::id),
        Some(&id)
    );
    Ok(())
}

#[test]
fn the_fhir_form_loads_the_native_endpoint_the_native_form_declares() -> TestResult {
    let toml = format!(
        "{NATIVE}\n[[endpoint]]\nid = \"node-a-eps\"\nnode = \"node-a\"\n\
         url = \"https://cdr-a.example.org/fhir\"\nconnection_type = \"hl7-fhir-rest\"\n\
         managing_organisation = \"org-a\"\ncategories = [\"{SUMMARIES}\"]\n\
         profiles = [\"{EPS}\"]\nehr_id_system = \"{EHR_ID_SYSTEM}\"\n"
    );
    assert_eq!(
        RegistrySnapshot::from_toml_str(&toml)?,
        load(&with(native_endpoint()))?
    );
    Ok(())
}

/// A `payloadType` coding in another system describes other content and is
/// no category; the text and the media type are read by no rule.
#[test]
fn a_payload_type_in_another_system_is_no_category() -> TestResult {
    let mut bundle = with(native_endpoint());
    native_resource(&mut bundle)["payloadType"] = json!([
        {"coding": [{"system": "http://terminology.hl7.org/CodeSystem/endpoint-payload-type", "code": "any"}]},
        {"coding": [{"system": CATEGORY_SYSTEM, "code": SUMMARIES}], "text": "EEHRxF"},
    ]);
    let snapshot = load(&bundle)?;
    let held = snapshot
        .native_endpoint(&endpoint("node-a-eps"))
        .ok_or("the native endpoint")?;
    assert_eq!(
        held.categories()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [SUMMARIES]
    );
    Ok(())
}

/// N33: without the identifier system the gateway could locate the member by
/// nothing it may send, so the endpoint is refused.
#[test]
fn a_native_endpoint_without_the_ehr_id_system_is_refused() {
    let mut bundle = with(native_endpoint());
    native_resource(&mut bundle)["extension"] =
        json!([{"url": PROFILE_EXTENSION, "valueCanonical": EPS}]);
    assert!(matches!(
        registry_refusal(&bundle),
        LoadError::NativeWithoutIdentifierSystem(refused) if refused == endpoint("node-a-eps")
    ));
}

#[test]
fn a_native_endpoint_without_a_category_is_refused() {
    let mut bundle = with(native_endpoint());
    native_resource(&mut bundle)["payloadType"] = json!([{"text": "EEHRxF"}]);
    assert!(matches!(
        registry_refusal(&bundle),
        LoadError::NativeWithout { endpoint: refused, key: "categories" }
            if refused == endpoint("node-a-eps")
    ));
}

#[test]
fn a_native_endpoint_without_a_profile_is_refused() {
    let mut bundle = with(native_endpoint());
    native_resource(&mut bundle)["extension"] =
        json!([{"url": EHR_ID_SYSTEM_EXTENSION, "valueUri": EHR_ID_SYSTEM}]);
    assert!(matches!(
        registry_refusal(&bundle),
        LoadError::NativeWithout { endpoint: refused, key: "profiles" }
            if refused == endpoint("node-a-eps")
    ));
}

/// The bare `hl7-fhir-rest` endpoint, with none of the keys, is refused as
/// before.
#[test]
fn an_hl7_fhir_rest_endpoint_without_the_native_keys_is_refused() {
    let mut bundle = fhir();
    resource(&mut bundle, NODE_A_PUB)["connectionType"] =
        json!({"system": mcsd::HL7_CONNECTION_TYPE_SYSTEM, "code": "hl7-fhir-rest"});
    assert!(matches!(
        registry_refusal(&bundle),
        LoadError::NativeWithoutIdentifierSystem(refused) if refused == endpoint("node-a-pub")
    ));
}

#[test]
fn a_consent_refusal_code_on_a_native_endpoint_is_misplaced() {
    let mut bundle = with(native_endpoint());
    if let Some(extensions) = native_resource(&mut bundle)["extension"].as_array_mut() {
        extensions.push(json!({"url": CONSENT_REFUSAL_CODE_EXTENSION, "valueCode": "CONSENT"}));
    }
    assert!(matches!(
        registry_refusal(&bundle),
        LoadError::MisplacedKey { endpoint: refused, key: "consent_refusal_codes" }
            if refused == endpoint("node-a-eps")
    ));
}

#[test]
fn a_category_on_a_query_endpoint_is_misplaced() {
    let mut bundle = fhir();
    resource(&mut bundle, NODE_A_PUB)["payloadType"] =
        json!([{"coding": [{"system": CATEGORY_SYSTEM, "code": SUMMARIES}]}]);
    assert!(matches!(
        registry_refusal(&bundle),
        LoadError::MisplacedKey { endpoint: refused, key: "categories" }
            if refused == endpoint("node-a-pub")
    ));
}

#[test]
fn an_ehr_id_system_on_a_query_endpoint_is_misplaced() {
    let mut bundle = fhir();
    resource(&mut bundle, NODE_A_PUB)["extension"] =
        json!([{"url": EHR_ID_SYSTEM_EXTENSION, "valueUri": EHR_ID_SYSTEM}]);
    assert!(matches!(
        registry_refusal(&bundle),
        LoadError::MisplacedKey { endpoint: refused, key: "ehr_id_system" }
            if refused == endpoint("node-a-pub")
    ));
}

#[test]
fn two_native_endpoints_of_a_node_for_one_category_are_refused() {
    let mut second = native_endpoint();
    second["fullUrl"] = json!("https://registry.example.org/fhir/Endpoint/node-a-eps2");
    second["resource"]["id"] = json!("node-a-eps2");
    second["resource"]["identifier"][0]["value"] = json!("node-a-eps2");
    second["resource"]["address"] = json!("https://cdr-a.example.org/fhir2");
    let mut bundle = with(native_endpoint());
    if let Some(listed) = resource(&mut bundle, ORG_A)["endpoint"].as_array_mut() {
        listed.push(json!({"reference": "Endpoint/node-a-eps2"}));
    }
    if let Some(entries) = bundle["entry"].as_array_mut() {
        entries.push(second);
    }
    assert!(matches!(
        registry_refusal(&bundle),
        LoadError::NativeCategoryTwice { category, .. } if category == SUMMARIES
    ));
}

#[test]
fn a_category_coding_without_a_code_is_refused() {
    let mut bundle = with(native_endpoint());
    native_resource(&mut bundle)["payloadType"] =
        json!([{"coding": [{"system": CATEGORY_SYSTEM}]}]);
    assert!(matches!(
        load(&bundle),
        Err(FhirFormError::Category(refused)) if refused == endpoint("node-a-eps")
    ));
}

#[test]
fn a_profile_extension_without_a_canonical_is_refused() {
    let mut bundle = with(native_endpoint());
    native_resource(&mut bundle)["extension"] = json!([
        {"url": PROFILE_EXTENSION, "valueUri": EPS},
        {"url": EHR_ID_SYSTEM_EXTENSION, "valueUri": EHR_ID_SYSTEM},
    ]);
    assert!(matches!(
        load(&bundle),
        Err(FhirFormError::Profile(refused)) if refused == endpoint("node-a-eps")
    ));
}

#[test]
fn an_ehr_id_system_extension_without_a_uri_is_refused() {
    let mut bundle = with(native_endpoint());
    native_resource(&mut bundle)["extension"] = json!([
        {"url": PROFILE_EXTENSION, "valueCanonical": EPS},
        {"url": EHR_ID_SYSTEM_EXTENSION, "valueCode": "not-a-uri"},
    ]);
    assert!(matches!(
        load(&bundle),
        Err(FhirFormError::EhrIdSystem { endpoint: refused, fault: IdentifierFault::NoValue })
            if refused == endpoint("node-a-eps")
    ));
}

#[test]
fn two_ehr_id_system_extensions_are_refused() {
    let mut bundle = with(native_endpoint());
    native_resource(&mut bundle)["extension"] = json!([
        {"url": PROFILE_EXTENSION, "valueCanonical": EPS},
        {"url": EHR_ID_SYSTEM_EXTENSION, "valueUri": EHR_ID_SYSTEM},
        {"url": EHR_ID_SYSTEM_EXTENSION, "valueUri": "urn:oid:2.999.4.2"},
    ]);
    assert!(matches!(
        load(&bundle),
        Err(FhirFormError::EhrIdSystem { endpoint: refused, fault: IdentifierFault::Repeated })
            if refused == endpoint("node-a-eps")
    ));
}

#[test]
fn the_harness_spells_the_native_keys_of_the_fhir_form() {
    assert_eq!(
        mcsd::HL7_CONNECTION_TYPE_SYSTEM,
        harness::HL7_CONNECTION_TYPE
    );
    assert_eq!(CATEGORY_SYSTEM, harness::CATEGORY);
    assert_eq!(PROFILE_EXTENSION, harness::PROFILE);
    assert_eq!(EHR_ID_SYSTEM_EXTENSION, harness::EHR_ID_SYSTEM);
}

fn member(name: &str) -> Member {
    Member {
        organisation: format!("org-{name}"),
        endpoint: format!("node-{name}-pub"),
        node: format!("node-{name}"),
        system_id: format!("cdr-{name}.example.org"),
        address: format!("https://cdr-{name}.example.org/openehr"),
    }
}

/// The native endpoint of `node-a`, managed by `org-fhir`, an organisation
/// that manages nothing else.
fn eps() -> Native<'static> {
    Native {
        endpoint: "node-a-eps",
        address: "https://cdr-a.example.org/fhir",
        manager: Some("org-fhir"),
        categories: &[SUMMARIES],
        profiles: &[EPS],
        ehr_id_system: Some(EHR_ID_SYSTEM),
    }
}

fn source(harness: &HarnessDirectory) -> Result<DirectorySource, Box<dyn Error>> {
    Ok(DirectorySource::new(DirectoryConfig {
        tls: Tls::default(),
        base: SecretUrl::new(harness.base()),
        credentials: Authentication::None,
        deadline: Duration::from_secs(5),
        pages: 200,
        bytes: 64 << 20,
        entries: 50_000,
    })?)
}

/// The organisation `org-fhir`, which manages the native endpoint and
/// operates nothing.
fn manager() -> Member {
    Member {
        organisation: "org-fhir".to_owned(),
        endpoint: "none".to_owned(),
        node: "none".to_owned(),
        system_id: "none.example.org".to_owned(),
        address: "https://none.example.org".to_owned(),
    }
}

/// A directory carries the native endpoint as the FHIR form does.
#[tokio::test]
async fn a_directory_carries_a_native_endpoint() -> TestResult {
    let harness = HarnessDirectory::start().await;
    let a = member("a");
    harness.publish(&[a.clone(), member("b")])?;
    harness.put_organization(manager().organisation_also_listing(&[])?);
    harness.put_organization(a.organisation_also_listing(&["Endpoint/node-a-eps"])?);
    harness.put_endpoint(a.native_endpoint(&eps())?);
    let read = source(&harness)?.read().await?;
    let held = read
        .snapshot()
        .native_endpoint(&endpoint("node-a-eps"))
        .ok_or("the native endpoint")?;
    assert_eq!(
        held.managing_organisation(),
        &"org-fhir".parse::<OrganisationId>()?
    );
    assert!(read.snapshot().endpoint(&endpoint("node-a-eps")).is_none());
    Ok(())
}

/// A refresh adds the native endpoint, and a later one that deletes it
/// removes it, though its manager then manages no endpoint: a directory's
/// registry keeps such an organisation.
#[tokio::test]
async fn a_refresh_adds_and_removes_a_native_endpoint() -> TestResult {
    let harness = HarnessDirectory::start().await;
    let a = member("a");
    harness.publish(&[a.clone(), member("b")])?;
    harness.put_organization(manager().organisation_also_listing(&[])?);
    let source = source(&harness)?;
    let read = source.read().await?;
    assert_eq!(read.snapshot().native_endpoints().count(), 0);

    harness.put_organization(a.organisation_also_listing(&["Endpoint/node-a-eps"])?);
    harness.put_endpoint(a.native_endpoint(&eps())?);
    let Refreshed::Changed(added) = source.refresh(read.content()).await? else {
        return Err("the refresh adding the native endpoint is applied".into());
    };
    let ids: Vec<&str> = added
        .snapshot()
        .native_endpoints()
        .map(|native| native.id().as_str())
        .collect();
    assert_eq!(ids, ["node-a-eps"]);

    harness.delete_endpoint("node-a-eps");
    let Refreshed::Changed(removed) = source.refresh(added.content()).await? else {
        return Err("the refresh removing the native endpoint is applied".into());
    };
    assert_eq!(removed.snapshot().native_endpoints().count(), 0);
    assert!(
        removed
            .snapshot()
            .organisation(&"org-fhir".parse()?)
            .is_some(),
        "the manager of no endpoint stays"
    );
    Ok(())
}
