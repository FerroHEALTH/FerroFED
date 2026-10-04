// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The registry read from a care services directory (§15.1, N21): the
//! federation's identifier systems select its members from a shared
//! directory, the testkit's harness directory standing in for one.

use std::error::Error;
use std::time::Duration;

use ferrofed_identity::directory::error::{FhirFormError, ReferenceFault};
use ferrofed_identity::directory::mcsd::{
    DirectoryConfig, DirectoryReadError, DirectorySource, Refreshed,
};
use ferrofed_identity::directory::{ENDPOINT_ID_SYSTEM, ORGANISATION_ID_SYSTEM};
use ferrofed_registry::id::{NodeId, OrganisationId};
use ferrofed_registry::secret::SecretUrl;
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
        base: SecretUrl::new(harness.base()),
        credentials: None,
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

#[tokio::test]
async fn a_refresh_into_a_broken_registry_is_refused_and_the_content_kept() -> TestResult {
    let harness = HarnessDirectory::start().await;
    harness.publish(&[member("a")])?;
    let source = source(&harness)?;
    let read = source.read().await?;
    harness.delete_endpoint("node-a-pub");
    let refreshed = source.refresh(read.content()).await?;
    assert!(matches!(refreshed, Refreshed::Refused(_)), "{refreshed:?}");
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
        base: SecretUrl::new(format!("{}/fhir", ferrofed_testkit::unreachable::BASE)),
        credentials: None,
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
        base: SecretUrl::new(harness.base()),
        credentials: None,
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

/// A refresh that deletes a member's endpoint while its organisation still
/// lists it leaves a listing of an endpoint the selection took dangling, and
/// is refused naming the organisation and the reference.
#[tokio::test]
async fn a_dangling_listing_of_a_selected_endpoint_is_refused() -> TestResult {
    let harness = HarnessDirectory::start().await;
    harness.publish(&[member("a"), member("b")])?;
    let source = source(&harness)?;
    let read = source.read().await?;
    harness.delete_endpoint("node-b-pub");
    let refreshed = source.refresh(read.content()).await?;
    let Refreshed::Refused(FhirFormError::OrganisationEndpoint {
        organisation,
        fault,
    }) = &refreshed
    else {
        return Err(format!("the dangling listing is refused: {refreshed:?}").into());
    };
    assert_eq!(&"org-b".parse::<OrganisationId>()?, organisation);
    assert_eq!(
        &ReferenceFault::Outside("Endpoint/node-b-pub".to_owned()),
        fault
    );
    Ok(())
}

/// An endpoint that leaves the selection while its organisation still lists
/// it cannot be told from one that was deleted, so the refresh is refused as
/// a dangling listing is.
#[tokio::test]
async fn a_listed_endpoint_that_leaves_the_selection_is_refused() -> TestResult {
    let harness = HarnessDirectory::start().await;
    let b = member("b");
    harness.publish(&[member("a"), b.clone()])?;
    let source = source(&harness)?;
    let read = source.read().await?;
    let mut unselected = b.endpoint()?;
    unselected.identifier.clear();
    harness.put_endpoint(unselected);
    let refreshed = source.refresh(read.content()).await?;
    assert!(
        matches!(
            &refreshed,
            Refreshed::Refused(FhirFormError::OrganisationEndpoint {
                fault: ReferenceFault::Outside(reference),
                ..
            }) if reference == "Endpoint/node-b-pub"
        ),
        "{refreshed:?}"
    );
    Ok(())
}
