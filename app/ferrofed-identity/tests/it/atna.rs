// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ITI-20 recorder of the XCPD localizer, against the testkit's harness
//! Audit Record Repository over TLS: every discovery's audit message is
//! stored, then delivered in order; a repository that cannot be reached
//! delays delivery without failing the discovery (ITI TF-2 §3.20.4.1.1);
//! and a spool that cannot store the message fails the discovery closed
//! (§14.1, ITI TF-2 §3.55.5.1).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeSet;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_identity::atna::RepositoryAudit;
use ferrofed_identity::localizer::{Localization, LocalizerError};
use ferrofed_testkit::atna::AuditRepository;
use ferrofed_testkit::xcpd::RespondingGateway;
use ihe_iti::atna::forwarder::Forwarder;
use ihe_iti::atna::message::AuditSource;
use ihe_iti::atna::repository::{Repository, TlsSettings};
use ihe_iti::atna::spool::{Bounds, Spool};
use ihe_iti::atna::syslog::Sender;
use url::Url;

use crate::support::PATIENT_VALUE;
use crate::xcpd::{COMMUNITY_A, holds, localize, localizer, node};

type TestResult = Result<(), Box<dyn Error>>;

const ROOMY: Bounds = Bounds {
    max_messages: 64,
    max_bytes: 1 << 20,
};

/// The recorder sending to `repository` over TLS, trusting its CA, through
/// `spool`.
fn recorder(repository: &AuditRepository, spool: Spool) -> Result<RepositoryAudit, Box<dyn Error>> {
    let tls = TlsSettings {
        roots: Some(repository.trust_roots().as_bytes().to_vec()),
        identity: None,
    };
    let address = Url::parse(&repository.url())?;
    let connection = Repository::tls(&address, &tls, Duration::from_secs(2))?;
    let sender = Sender::new("gateway.example.org", "ferrofed", "4242")?;
    let source = AuditSource {
        id: "gateway.example.org".to_owned(),
        enterprise_site: None,
    };
    Ok(RepositoryAudit::new(
        Forwarder::new(spool, connection),
        sender,
        source,
    ))
}

#[tokio::test]
async fn each_discovery_reaches_the_repository_as_one_iti_20_message() -> TestResult {
    let repository = AuditRepository::start().await?;
    let audit = Arc::new(recorder(&repository, Spool::in_memory(ROOMY))?);
    let stub = RespondingGateway::answering(holds(COMMUNITY_A)).await;
    let localizer = localizer(&[&stub])?.audited(audit.clone());

    match localize(&localizer).await? {
        Localization::Candidates(named) => assert_eq!(BTreeSet::from([node("node-a")?]), named),
        other => return Err(format!("a candidate set: {other:?}").into()),
    }
    let messages = repository.wait_for(1, Duration::from_secs(5)).await;
    assert_eq!(1, messages.len(), "one message per exchange");
    let message = &messages[0];
    assert!(
        message.starts_with("<85>1 "),
        "PRI <85> (ITI TF-2 §3.20.4.1.2): {message}"
    );
    assert!(
        message.contains(" gateway.example.org ferrofed 4242 IHE+RFC-3881 - <?xml"),
        "the header and no structured data: {message}"
    );
    assert!(message.contains("<EventTypeCode csd-code=\"ITI-55\""));
    assert!(message.contains("<ParticipantObjectQuery>"));
    assert!(
        !message.contains(PATIENT_VALUE),
        "the identifier travels only base64-encoded inside the query"
    );
    assert_eq!(0, audit.status().depth.messages, "delivered, not held");
    Ok(())
}

#[tokio::test]
async fn a_repository_that_is_down_delays_delivery_and_fails_nothing() -> TestResult {
    let repository = AuditRepository::start().await?;
    repository.set_up(false);
    let directory = tempfile::tempdir()?;
    let spool = Spool::open(&directory.path().join("spool"), ROOMY)?;
    let audit = Arc::new(recorder(&repository, spool)?);
    let stub = RespondingGateway::answering(holds(COMMUNITY_A)).await;
    let localizer = localizer(&[&stub])?.audited(audit.clone());

    for _ in 0..3 {
        assert!(
            matches!(localize(&localizer).await?, Localization::Candidates(_)),
            "an outage of the repository is stored, not refused (§3.20.4.1.1)"
        );
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(repository.messages().is_empty());
    assert_eq!(3, audit.status().depth.messages, "every message is held");

    repository.set_up(true);
    let messages = repository.wait_for(3, Duration::from_secs(10)).await;
    assert_eq!(
        3,
        messages.len(),
        "the spool drains once the repository is back"
    );
    assert_eq!(0, audit.status().depth.messages);
    assert!(audit.status().reachable);
    Ok(())
}

// conformance: CP-5
#[tokio::test]
async fn a_full_spool_fails_the_discovery_closed() -> TestResult {
    let repository = AuditRepository::start().await?;
    repository.set_up(false);
    let spool = Spool::in_memory(Bounds {
        max_messages: 1,
        max_bytes: 1 << 20,
    });
    let audit = Arc::new(recorder(&repository, spool)?);
    let stub = RespondingGateway::answering(holds(COMMUNITY_A)).await;
    let localizer = localizer(&[&stub])?.audited(audit);

    assert!(matches!(
        localize(&localizer).await?,
        Localization::Candidates(_)
    ));
    match localize(&localizer).await? {
        Localization::Unavailable(error @ LocalizerError::AuditFailed(_)) => {
            let mut text = error.to_string();
            let mut cause = Error::source(&error);
            while let Some(link) = cause {
                text.push_str(": ");
                text.push_str(&link.to_string());
                cause = link.source();
            }
            assert!(text.contains("spool is full"), "{text}");
            assert!(!text.contains(PATIENT_VALUE), "{text}");
            Ok(())
        }
        other => Err(format!("no answer without its audit record (§14.1): {other:?}").into()),
    }
}
