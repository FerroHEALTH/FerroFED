// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Test support for the FerroFED suites, consumed as a path-only
//! dev-dependency so `cargo package` strips it.
//!
//! It reads the pin matrix, so a crate's version constant can be asserted
//! against the single source of truth, and it holds the harness of the
//! conformance tracks (§16):
//!
//! - [`atna`]: a harness ATNA Audit Record Repository that reads ITI-20
//!   syslog over TLS (#418);
//! - [`atna_feed`]: a harness Audit Record Repository that takes the BALP
//!   `AuditEvent` records ITI-20 posts over the FHIR Feed (#486);
//! - [`containers`]: the two CDR products behind the `FERROFED_E2E` gate,
//!   pinned by digest;
//! - [`proxy`]: the capturing and fault proxy in front of each node, whose
//!   journal the tests read;
//! - [`leak`]: the track 10 oracle over that journal, which searches every
//!   carrier for an identifier and its fragments, raw and percent-decoded;
//! - [`dpop`]: the check a test device makes of a `DPoP` proof, and the
//!   nonce challenge a node answers with (#439);
//! - [`fapi`]: the harness FAPI 2.0 authorization server, the token
//!   endpoint held to the profile with its RFC 8414 metadata beside it
//!   (#497);
//! - [`issuer`]: a test issuer that mints RFC 9068 access tokens and serves
//!   its key set (#80);
//! - [`mcsd`]: the harness care services directory, a test device that
//!   answers ITI-90 and ITI-91 over the registry members a test puts in it
//!   (#86);
//! - [`mock`]: the wiremock server every suite stands its nodes up with,
//!   dropped outside the test's runtime;
//! - [`nuts`]: the harness Nuts node, a test device that answers the
//!   GF-Authentication access token request (Nuts RFC021) with a
//!   `DPoP`-bound token for a valid Verifiable Presentation (#88);
//! - [`node_profile`]: the Federation-Node profile checks, which exercise
//!   what a member CDR's ITS-REST interface shows of the node obligations of
//!   §16.2 and record a finding per obligation for the report (#93);
//! - [`nvi`]: a stub NVI Localization Service answering the GF-Localization
//!   search of the Dutch Generic Functions (#87);
//! - [`oauth`]: the harness OAuth 2.0 token endpoint, which verifies the
//!   gateway's client assertion against its published JWK Set and issues
//!   the token a mock node then requires (#81), exchanges a caller's token
//!   (RFC 8693) and binds a token to a `DPoP` key (RFC 9449) (#439);
//! - [`otlp`]: an in-process OTLP/gRPC trace collector, which keeps every
//!   span the gateway exports (#437);
//! - [`pdq`]: the harness PDQm Supplier, a test device that answers ITI-78
//!   searches by identifier and ITI-119 matches from synthetic Patients in the
//!   `urn:oid:2.999` example arc (#487);
//! - [`pix`]: the harness PIX Manager, a test device that answers ITI-83 from
//!   what an ITI-104 feed delivered (#47);
//! - [`pmir`]: the harness Patient Identity Registry, a test device that
//!   takes ITI-94 subscriptions and sends ITI-93 messages to them (#147);
//! - [`tls`]: a mutual-TLS front that puts any harness server behind
//!   `https` with client authentication (#507);
//! - [`unreachable`](mod@unreachable): a base URL no connection can
//!   reach, the unreachable node of a test;
//! - [`seed`]: the synthetic seed builder, which writes over ITS-REST alone,
//!   feeds the PIX Manager over ITI-104, and names patients only inside the
//!   `urn:oid:2.999` example arc;
//! - [`xcpd`]: a stub XCPD Responding Gateway answering ITI-55 (#85);
//! - [`mitz`](mod@mitz): a stub Mitz answering the closed authorization
//!   question of the Dutch Generic Functions, the consent pre-filter of
//!   Annex B §B.6 (#475).
#![doc(test(attr(deny(warnings))))]

pub mod atna;
pub mod atna_feed;
pub mod containers;
pub mod dpop;
pub mod fapi;
pub mod issuer;
pub mod leak;
pub mod mcsd;
pub mod mitz;
pub mod mock;
pub mod node_profile;
pub mod nuts;
pub mod nvi;
pub mod oauth;
pub mod otlp;
pub mod pdq;
pub mod pix;
pub mod pmir;
pub mod proxy;
pub mod seed;
pub mod tls;
pub mod unreachable;
pub mod xcpd;

use std::fmt;
use std::path::PathBuf;

/// The pin matrix at `docs/VERSIONS.md`, relative to this crate's manifest.
const MATRIX: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/VERSIONS.md");

/// A pin could not be read from the matrix.
#[derive(Debug)]
pub enum PinError {
    /// The matrix file could not be read.
    Read {
        /// The path that was tried.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// No table row has the requested item in its first cell.
    Missing {
        /// The item that was looked up.
        item: String,
    },
}

impl fmt::Display for PinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, .. } => {
                write!(f, "cannot read the pin matrix at {}", path.display())
            }
            Self::Missing { item } => write!(f, "the pin matrix has no row for {item}"),
        }
    }
}

impl std::error::Error for PinError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Missing { .. } => None,
        }
    }
}

/// Returns the first token of the `Pin` cell of the matrix row whose first
/// cell is `item`, with backticks removed.
///
/// This mirrors what `scripts/checks/versions.sh` reads, so a crate constant
/// and the guard agree on the same value.
///
/// # Errors
///
/// Returns [`PinError::Read`] when `docs/VERSIONS.md` cannot be read and
/// [`PinError::Missing`] when no row carries `item`.
pub fn matrix_pin(item: &str) -> Result<String, PinError> {
    let path = PathBuf::from(MATRIX);
    let text = std::fs::read_to_string(&path).map_err(|source| PinError::Read {
        path: path.clone(),
        source,
    })?;
    text.lines()
        .find_map(|line| {
            let mut cells = line
                .split('|')
                .skip(1)
                .map(|c| c.replace('`', "").trim().to_owned());
            let key = cells.next()?;
            let pin = cells.next()?;
            (key == item).then(|| pin.split_whitespace().next().unwrap_or_default().to_owned())
        })
        .ok_or_else(|| PinError::Missing {
            item: item.to_owned(),
        })
}
