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

use ferrofed_identity::ihe::audit::atna::RepositoryAudit;
use ferrofed_identity::role::localizer::{Localization, Localizer as _, LocalizerError};
use ferrofed_testkit::atna::AuditRepository;
use ferrofed_testkit::xcpd::RespondingGateway;
use ihe_iti::atna::forwarder::Forwarder;
use ihe_iti::atna::message::AuditSource;
use ihe_iti::atna::repository::{Repository, Timeouts, TlsSettings};
use ihe_iti::atna::spool::{Bounds, Spool};
use ihe_iti::atna::syslog::Sender;
use url::Url;

use crate::support::{CALLER_AUDIENCE, CALLER_ISSUER, CALLER_SUBJECT, PATIENT_VALUE, caller};
use crate::timing;
use crate::xcpd::{COMMUNITY_A, holds, localize, localizer, node};

type TestResult = Result<(), Box<dyn Error>>;

const ROOMY: Bounds = Bounds {
    max_messages: 64,
    max_bytes: 1 << 20,
    write_timeout: Duration::from_secs(10),
};

/// The timeouts towards the harness repository. The send timeout, which
/// also bounds the TLS handshake, is the slack a loaded host gets, so a
/// handshake under load finishes within it.
const TIMEOUTS: Timeouts = Timeouts {
    connect: Duration::from_secs(2),
    send: timing::SLACK,
};

const RETRY_MAX: Duration = Duration::from_millis(500);

/// The longest the forwarder may take to deliver `messages` over one new
/// connection under [`TIMEOUTS`]: the connection, the TLS handshake, and a
/// write and a flush for each message.
const fn delivery(messages: u32) -> Duration {
    TIMEOUTS
        .connect
        .saturating_add(TIMEOUTS.send)
        .saturating_add(TIMEOUTS.send.saturating_mul(2).saturating_mul(messages))
}

/// The recorder sending to `repository` over TLS, trusting its CA, through
/// `spool`.
fn recorder(repository: &AuditRepository, spool: Spool) -> Result<RepositoryAudit, Box<dyn Error>> {
    let tls = TlsSettings {
        roots: Some(repository.trust_roots().as_bytes().to_vec()),
        identity: None,
    };
    let address = Url::parse(&repository.url())?;
    let connection = Repository::tls(&address, &tls, TIMEOUTS)?;
    let sender = Sender::new("gateway.example.org", "ferrofed", "4242")?;
    let source = AuditSource {
        id: "gateway.example.org".to_owned(),
        enterprise_site: None,
    };
    Ok(RepositoryAudit::new(
        Forwarder::new(spool, connection, RETRY_MAX),
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
    let messages = repository.wait_for(1, timing::within(delivery(1))).await;
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
async fn a_discovery_made_for_a_caller_names_them_as_its_human_requestor() -> TestResult {
    let repository = AuditRepository::start().await?;
    let audit = Arc::new(recorder(&repository, Spool::in_memory(ROOMY))?);
    let stub = RespondingGateway::answering(holds(COMMUNITY_A)).await;
    let localizer = localizer(&[&stub])?.audited(audit.clone());
    let answer = localizer
        .localize(
            &crate::xcpd::patient()?,
            &crate::xcpd::members()?,
            &caller(),
            std::time::Instant::now() + Duration::from_secs(2),
        )
        .await;
    assert!(matches!(answer, Localization::Candidates(_)), "{answer:?}");
    let messages = repository.wait_for(1, timing::within(delivery(1))).await;
    assert_eq!(1, messages.len(), "one message per exchange");
    // NOTE: ITI TF-2 §3.55.5.1.1 Human Requestor UserID is the human's identity, and IUA
    // ITI TF-2 §3.72.5.1 writes the JWT's aud, sub and iss into UserName.
    let requestor = format!(
        "<ActiveParticipant UserID=\"{CALLER_SUBJECT}\" UserName=\"{CALLER_AUDIENCE}&lt;{CALLER_SUBJECT}@{CALLER_ISSUER}&gt;\" UserIsRequestor=\"true\">"
    );
    assert!(messages[0].contains(&requestor), "{}", messages[0]);
    assert!(
        messages[0]
            .contains("UserIsRequestor=\"false\" NetworkAccessPointID=\"gateway.example.org\""),
        "one requestor: the caller, never the gateway too (DICOM PS3.15 A.5.2)"
    );
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
    // The forwarder may be in a failing attempt: it ends that and its
    // backoff, then delivers the three messages over a new connection.
    let recovery = TIMEOUTS
        .connect
        .saturating_add(TIMEOUTS.send)
        .saturating_add(RETRY_MAX)
        .saturating_add(delivery(3));
    let messages = repository.wait_for(3, timing::within(recovery)).await;
    assert_eq!(
        3,
        messages.len(),
        "the spool drains once the repository is back"
    );
    assert_eq!(0, audit.status().depth.messages);
    assert!(audit.status().reachable);
    Ok(())
}

#[tokio::test]
async fn a_repository_that_hangs_in_the_handshake_is_given_up_and_retried() -> TestResult {
    let repository = AuditRepository::start().await?;
    repository.set_stalled(true);
    let audit = Arc::new(recorder(&repository, Spool::in_memory(ROOMY))?);
    let stub = RespondingGateway::answering(holds(COMMUNITY_A)).await;
    let localizer = localizer(&[&stub])?.audited(audit.clone());

    let asked = std::time::Instant::now();
    assert!(matches!(
        localize(&localizer).await?,
        Localization::Candidates(_)
    ));
    assert!(
        asked.elapsed() < TIMEOUTS.send,
        "the discovery never waits on the repository"
    );
    let deadline =
        std::time::Instant::now() + timing::within(TIMEOUTS.connect.saturating_add(TIMEOUTS.send));
    while audit.status().retries == 0 && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let stalled = audit.status();
    assert!(
        stalled.retries >= 1,
        "the handshake is given up within the send timeout"
    );
    assert!(!stalled.reachable);
    assert_eq!(1, stalled.depth.messages, "the message stays spooled");

    repository.set_stalled(false);
    // The forwarder may be in a stalled handshake: it gives that up, backs
    // off, then delivers over a new connection.
    let recovery = TIMEOUTS
        .send
        .saturating_add(RETRY_MAX)
        .saturating_add(delivery(1));
    let messages = repository.wait_for(1, timing::within(recovery)).await;
    assert_eq!(1, messages.len(), "delivered once the repository answers");
    assert_eq!(0, audit.status().depth.messages);
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
        write_timeout: Duration::from_secs(10),
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

/// A forwarder started on a runtime that has since shut down is started
/// again on the next runtime, so the spool never stays undelivered (ITI
/// TF-2 §3.20.4.1.1).
#[tokio::test(flavor = "multi_thread")]
async fn a_forwarder_whose_runtime_ended_is_started_again() -> TestResult {
    let repository = AuditRepository::start().await?;
    let tls = TlsSettings {
        roots: Some(repository.trust_roots().as_bytes().to_vec()),
        identity: None,
    };
    let connection = Repository::tls(&Url::parse(&repository.url())?, &tls, TIMEOUTS)?;
    let sender = Sender::new("gateway.example.org", "ferrofed", "4242")?;
    let forwarder = Forwarder::new(Spool::in_memory(ROOMY), connection, RETRY_MAX);
    let audit = Arc::new(RepositoryAudit::new(
        Arc::clone(&forwarder),
        sender.clone(),
        AuditSource {
            id: "gateway.example.org".to_owned(),
            enterprise_site: None,
        },
    ));
    let short = Arc::clone(&audit);
    std::thread::spawn(move || -> Result<(), std::io::Error> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async {
            short.start();
            tokio::task::yield_now().await;
        });
        Ok(())
    })
    .join()
    .map_err(|_panic| "the short-lived runtime's thread")??;
    audit.start();
    let frame = sender.frame(
        jiff::Timestamp::now(),
        &secrecy::SecretSlice::from(b"<AuditMessage/>".to_vec()),
    );
    forwarder.submit(frame).await?;
    let messages = repository.wait_for(1, timing::within(delivery(1))).await;
    assert_eq!(1, messages.len(), "the restarted forwarder delivers");
    Ok(())
}
