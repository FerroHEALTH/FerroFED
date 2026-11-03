// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The audit recorder that sends each ITI-55 audit message to an ATNA Audit
//! Record Repository over ITI-20 (ITI TF-2 §3.20, §3.55.5.1.1).
//!
//! Each event is written as its DICOM PS3.15 audit message in an RFC 5424
//! syslog message, stored in the spool, and delivered from there in order
//! by `ihe_iti`'s forwarder. The event counts as recorded once it is stored:
//! a repository that cannot be reached delays its delivery (ITI TF-2
//! §3.20.4.1.1), and a spool that is full, cannot be written, or does not
//! store the message within its write bound refuses the event, which fails
//! the discovery closed.
//!
//! The message names the patient, inside the base64 query parameters, so it
//! leaves this module only for the spool and the repository connection; no
//! log line and no error carries it.

use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use ihe_atna::atna::forwarder::Forwarder;
use ihe_atna::atna::message::{AccessPoint, AuditSource};
use ihe_atna::atna::syslog::Sender;
use ihe_iti::xcpd::audit::{AuditError, AuditEvent, AuditRecorder};

use crate::ihe::audit::delivery::Delivery;

/// The recorder over one repository and its spool.
pub struct RepositoryAudit {
    delivery: Delivery,
    sender: Sender,
    source: AuditSource,
    host: AccessPoint,
}

impl fmt::Debug for RepositoryAudit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RepositoryAudit")
            .field("sender", &self.sender)
            .field("source", &self.source)
            .field("delivery", &self.delivery)
            .finish_non_exhaustive()
    }
}

impl RepositoryAudit {
    /// The recorder that writes as `sender` and names `source` in every
    /// message, delivering through `forwarder`.
    ///
    /// The host the gateway runs on, the source's network access point, is
    /// the sender's `HOSTNAME`: an IP address when it reads as one, and a
    /// machine name otherwise.
    #[must_use]
    pub fn new(forwarder: Arc<Forwarder>, sender: Sender, source: AuditSource) -> Self {
        let hostname = sender.hostname().to_owned();
        let type_code = if hostname.parse::<std::net::IpAddr>().is_ok() {
            "2"
        } else {
            "1"
        };
        Self {
            delivery: Delivery::new(forwarder),
            sender,
            source,
            host: AccessPoint {
                type_code,
                id: hostname,
            },
        }
    }

    /// The delivery of the spool to the repository.
    #[must_use]
    pub fn delivery(&self) -> &Delivery {
        &self.delivery
    }
}

#[async_trait]
impl AuditRecorder for RepositoryAudit {
    async fn record(&self, event: AuditEvent) -> Result<(), AuditError> {
        self.delivery.start();
        let xml = event
            .message(&self.source, &self.host)
            .to_xml()
            .map_err(|error| AuditError(Box::new(error)))?;
        let frame = self.sender.frame(jiff::Timestamp::now(), &xml);
        self.delivery
            .forwarder()
            .submit(frame)
            .await
            .map_err(|error| AuditError(Box::new(error)))
    }
}
