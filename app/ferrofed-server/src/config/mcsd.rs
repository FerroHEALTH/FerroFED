// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mCSD care services directory, `[registry.mcsd]` (§15.1, Annex A.5).

use std::path::PathBuf;

use ferrofed_registry::secret::{Secret, SecretUrl};
use serde::Deserialize;

use crate::config::Credentials;

/// The mCSD care services directory the registry is read from.
///
/// Its `Organization`s and `Endpoint`s are read with ITI-90 and checked as
/// the registry document in FHIR form is; every `refresh_interval_s` the
/// changes since the last read are asked for with ITI-91 and checked again
/// before they replace the running registry. Each read or refresh ends at
/// its deadline and its caps on pages, bytes and entries, so a faulty
/// directory can neither hold it nor fill the gateway's memory (no
/// specification governs these limits: our own design).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct McsdDirectory {
    /// The directory's FHIR base URL, `http` or `https`, with no user name
    /// or password.
    pub url: SecretUrl,
    /// How the gateway authenticates to the directory, when the transport
    /// does not: a bearer token or basic credentials.
    pub credentials: Option<Credentials>,
    /// The gateway's client certificate chain and private key, PEM, for
    /// mutual TLS with the directory, inline or through
    /// `client_identity_file`.
    pub client_identity: Option<Secret>,
    /// A file holding the client identity, read at boot.
    pub client_identity_file: Option<PathBuf>,
    /// A file of PEM trust roots the directory's certificate chains to,
    /// beside the platform's.
    pub trust_roots_file: Option<PathBuf>,
    /// How often the changes are asked for, in seconds.
    pub refresh_interval_s: u64,
    /// How long one whole read or refresh may take, every page of both
    /// resource types included, in milliseconds.
    pub deadline_ms: u64,
    /// The most pages one read or refresh may read.
    pub max_pages: usize,
    /// The most bytes of answer bodies one read or refresh may read.
    pub max_bytes: usize,
    /// The most Bundle entries one read or refresh may read.
    pub max_entries: usize,
}

impl Default for McsdDirectory {
    fn default() -> Self {
        Self {
            url: SecretUrl::default(),
            credentials: None,
            client_identity: None,
            client_identity_file: None,
            trust_roots_file: None,
            refresh_interval_s: 300,
            deadline_ms: 30_000,
            max_pages: 200,
            max_bytes: 64 << 20,
            max_entries: 50_000,
        }
    }
}
