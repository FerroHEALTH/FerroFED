// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The registry read from a care services directory (§15.1, N21): the
//! federation's identifier systems select its members from a shared
//! directory, the testkit's harness directory standing in for one.

use std::error::Error;
use std::time::Duration;

use ferrofed_identity::fhir::{Authentication, Tls};
use ferrofed_identity::ihe::mcsd::error::FhirFormError;
use ferrofed_identity::ihe::mcsd::source::{
    DirectoryConfig, DirectoryReadError, DirectorySource, Refreshed,
};
use ferrofed_identity::ihe::mcsd::{ENDPOINT_ID_SYSTEM, ORGANISATION_ID_SYSTEM};
use ferrofed_registry::error::LoadError;
use ferrofed_registry::id::{EndpointId, NodeId, OrganisationId};
use ferrofed_registry::secret::SecretUrl;
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_testkit::mcsd::{self, HarnessDirectory, Member};

type TestResult = Result<(), Box<dyn Error>>;

fn member(name: &str) -> Member {
    Member {
        organisation: format!("org-{name}"),
        endpoint: format!("node-{name}-pub"),
        node: format!("node-{name}"),
        system_id: format!("cdr-{name}.example.org"),
        address: format!("https://cdr-{name}.example.org/openehr"),
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

#[test]
fn the_harness_spells_the_identifier_systems_of_the_fhir_form() {
    assert_eq!(ORGANISATION_ID_SYSTEM, mcsd::ORGANISATION_ID);
    assert_eq!(ENDPOINT_ID_SYSTEM, mcsd::ENDPOINT_ID);
}

/// A directory shared with other services holds the IG's own examples, XCA
/// and DICOM endpoints among them; only the federation's resources become
/// members, and the rest never reaches the connection-type rule.
#[tokio::test]
async fn only_the_federations_resources_of_a_shared_directory_are_members() -> TestResult {
    let harness = HarnessDirectory::start().await;
    harness.publish_examples()?;
    harness.publish(&[member("a"), member("b")])?;
    let read = source(&harness)?.read().await?;
    assert_eq!((2, 2), read.content().len());
    let nodes: Vec<&NodeId> = read
        .snapshot()
        .nodes()
        .map(ferrofed_registry::snapshot::Node::id)
        .collect();
    assert_eq!(
        vec![&"node-a".parse::<NodeId>()?, &"node-b".parse::<NodeId>()?],
        nodes
    );
    Ok(())
}

#[tokio::test]
async fn a_member_relying_on_hl7_fhir_rest_is_no_registry() -> TestResult {
    let harness = HarnessDirectory::start().await;
    let a = member("a");
    harness.put_organization(a.organisation()?);
    harness.put_endpoint(a.endpoint_with_connection_type(
        "http://terminology.hl7.org/CodeSystem/endpoint-connection-type",
        "hl7-fhir-rest",
    )?);
    let refused = source(&harness)?.read().await;
    assert!(
        matches!(refused, Err(DirectoryReadError::Registry(_))),
        "{refused:?}"
    );
    Ok(())
}

/// A refresh that deletes the one member's endpoint leaves no node, which no
/// registry admits, and is refused with the content read kept.
#[tokio::test]
async fn a_refresh_into_a_broken_registry_is_refused_and_the_content_kept() -> TestResult {
    let harness = HarnessDirectory::start().await;
    harness.publish(&[member("a")])?;
    let source = source(&harness)?;
    let read = source.read().await?;
    harness.delete_endpoint("node-a-pub");
    let refreshed = source.refresh(read.content()).await?;
    assert!(
        matches!(
            refreshed,
            Refreshed::Refused(FhirFormError::Registry(LoadError::NoNode))
        ),
        "{refreshed:?}"
    );
    assert_eq!(
        (1, 1),
        read.content().len(),
        "the content read is unchanged"
    );
    Ok(())
}

#[tokio::test]
async fn a_directory_that_does_not_answer_is_an_exchange_error() -> TestResult {
    let source = DirectorySource::new(DirectoryConfig {
        tls: Tls::default(),
        base: SecretUrl::new(format!("{}/fhir", ferrofed_testkit::unreachable::BASE)),
        credentials: Authentication::None,
        deadline: Duration::from_secs(2),
        pages: 200,
        bytes: 64 << 20,
        entries: 50_000,
    })?;
    let refused = source.read().await;
    let Err(DirectoryReadError::Exchange(error)) = &refused else {
        return Err(format!("an exchange error: {refused:?}").into());
    };
    assert!(!error.answered());
    Ok(())
}

/// A refresh asks ITI-91 from one minute before the `Date` the directory
/// stamped its read with, on the directory's own clock.
#[tokio::test]
async fn a_refresh_asks_from_a_minute_before_the_directorys_clock() -> TestResult {
    let harness = HarnessDirectory::start().await;
    harness.publish(&[member("a")])?;
    let source = source(&harness)?;
    let read = source.read().await?;
    let refreshed = source.refresh(read.content()).await?;
    assert!(
        matches!(refreshed, Refreshed::Unchanged(_)),
        "{refreshed:?}"
    );
    let requests = harness.requests().await;
    assert!(
        requests
            .iter()
            .any(|request| request.ends_with("_history?_since=2026-01-01T01%3A59%3A00Z")),
        "two changes move the harness clock to 02:00: {requests:?}"
    );
    Ok(())
}

/// A directory whose answer runs past a cap gives no registry: the read is an
/// exchange error that says it ran out of its budget.
#[tokio::test]
async fn a_directory_past_a_cap_gives_no_registry() -> TestResult {
    let harness = HarnessDirectory::start().await;
    harness.publish(&[member("a"), member("b")])?;
    let source = DirectorySource::new(DirectoryConfig {
        tls: Tls::default(),
        base: SecretUrl::new(harness.base()),
        credentials: Authentication::None,
        deadline: Duration::from_secs(5),
        pages: 200,
        bytes: 64 << 20,
        entries: 3,
    })?;
    let refused = source.read().await;
    let Err(DirectoryReadError::Exchange(error)) = &refused else {
        return Err(format!("an exchange error: {refused:?}").into());
    };
    assert!(error.exceeded(), "{error:?}");
    Ok(())
}

/// The IG's XCA query endpoint, an endpoint of another service the shared
/// directory holds.
const XCA_QUERY: &str = "Endpoint/ex-endpointXCAquery";

/// A member organisation of a shared directory that also lists another
/// service's endpoint, one the selection did not take, is read: the listing is
/// ignored, never a refusal (no specification governs this: our own design).
#[tokio::test]
async fn a_member_listing_an_endpoint_outside_the_selection_is_read() -> TestResult {
    let harness = HarnessDirectory::start().await;
    harness.publish_examples()?;
    let a = member("a");
    harness.publish(&[a.clone(), member("b")])?;
    harness.put_organization(a.organisation_also_listing(&[XCA_QUERY, "Endpoint/node-gone"])?);
    let read = source(&harness)?.read().await?;
    let nodes: Vec<&NodeId> = read
        .snapshot()
        .nodes()
        .map(ferrofed_registry::snapshot::Node::id)
        .collect();
    assert_eq!(
        vec![&"node-a".parse::<NodeId>()?, &"node-b".parse::<NodeId>()?],
        nodes
    );
    Ok(())
}

/// A refresh in which a member organisation starts listing another service's
/// endpoint changes the content, and is never refused for it.
#[tokio::test]
async fn a_refresh_that_adds_a_listing_outside_the_selection_is_applied() -> TestResult {
    let harness = HarnessDirectory::start().await;
    harness.publish_examples()?;
    let a = member("a");
    harness.publish(&[a.clone(), member("b")])?;
    let source = source(&harness)?;
    let read = source.read().await?;
    harness.put_organization(a.organisation_also_listing(&[XCA_QUERY])?);
    let refreshed = source.refresh(read.content()).await?;
    let Refreshed::Changed(changed) = &refreshed else {
        return Err(format!("the changed organisation is read: {refreshed:?}").into());
    };
    assert_eq!(read.snapshot(), changed.snapshot());
    Ok(())
}

/// The nodes of a registry, in id order.
fn nodes(snapshot: &RegistrySnapshot) -> Vec<NodeId> {
    snapshot
        .nodes()
        .map(ferrofed_registry::snapshot::Node::id)
        .cloned()
        .collect()
}

/// The registry a refresh changed into, or the refresh's own outcome as the
/// error.
fn changed(refreshed: &Refreshed) -> Result<&RegistrySnapshot, Box<dyn Error>> {
    match refreshed {
        Refreshed::Changed(materialised) => Ok(materialised.snapshot()),
        other => Err(format!("the refresh changes the registry: {other:?}").into()),
    }
}

/// A refresh that reads an ITI-91 `DELETE` of a member's endpoint drops it,
/// and its node with it, while the organisation still lists it: the listing
/// names no member and is ignored, as at a start. The directory deleted the
/// endpoint, so the gateway stops routing to it (no specification governs
/// this: our own design).
#[tokio::test]
async fn a_refresh_that_deletes_a_listed_endpoint_drops_it() -> TestResult {
    let harness = HarnessDirectory::start().await;
    harness.publish(&[member("a"), member("b")])?;
    let source = source(&harness)?;
    let read = source.read().await?;
    harness.delete_endpoint("node-b-pub");
    let refreshed = source.refresh(read.content()).await?;
    let snapshot = changed(&refreshed)?;
    assert_eq!(vec!["node-a".parse::<NodeId>()?], nodes(snapshot));
    assert!(
        snapshot
            .endpoint(&"node-b-pub".parse::<EndpointId>()?)
            .is_none(),
        "the deleted endpoint left the registry"
    );
    assert!(
        snapshot
            .organisation(&"org-b".parse::<OrganisationId>()?)
            .is_some(),
        "its organisation stays a member"
    );
    Ok(())
}

/// An endpoint that loses the federation's identifier leaves the selection,
/// and the refresh drops it as a deletion does, its organisation's listing
/// ignored.
#[tokio::test]
async fn a_listed_endpoint_that_leaves_the_selection_is_dropped() -> TestResult {
    let harness = HarnessDirectory::start().await;
    let b = member("b");
    harness.publish(&[member("a"), b.clone()])?;
    let source = source(&harness)?;
    let read = source.read().await?;
    let mut unselected = b.endpoint()?;
    unselected.identifier.clear();
    harness.put_endpoint(unselected);
    let refreshed = source.refresh(read.content()).await?;
    assert_eq!(
        vec!["node-a".parse::<NodeId>()?],
        nodes(changed(&refreshed)?)
    );
    Ok(())
}

/// A start and a refresh over the same directory content give the same
/// registry: a deletion, an endpoint that left the selection, another
/// service's endpoint and a listing of nothing at all are each read alike.
#[tokio::test]
async fn a_start_and_a_refresh_over_the_same_content_agree() -> TestResult {
    let harness = HarnessDirectory::start().await;
    harness.publish_examples()?;
    let (a, b, c) = (member("a"), member("b"), member("c"));
    harness.publish(&[a.clone(), b, c.clone()])?;
    let source = source(&harness)?;
    let read = source.read().await?;

    harness.delete_endpoint("node-b-pub");
    let mut unselected = c.endpoint()?;
    unselected.identifier.clear();
    harness.put_endpoint(unselected);
    harness.put_organization(a.organisation_also_listing(&[XCA_QUERY, "Endpoint/node-gone"])?);
    let refreshed = source.refresh(read.content()).await?;
    let started = source.read().await?;

    assert_eq!(started.snapshot(), changed(&refreshed)?);
    assert_eq!(vec!["node-a".parse::<NodeId>()?], nodes(started.snapshot()));
    Ok(())
}
