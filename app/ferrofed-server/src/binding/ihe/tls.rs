// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The TLS material a table names for the service it reaches (#507).
//!
//! `[[pixm.manager]]`, `[pdqm]`, `[pmir]` and `[registry.mcsd]` take the keys
//! `[xcpd]` takes: `client_identity`, inline, or `client_identity_file`, the
//! gateway's client certificate chain and private key in PEM for mutual TLS,
//! held as a secret; and `trust_roots_file`, a PEM bundle of trust roots
//! beside the platform's. Every file is read once, at load. No specification
//! governs the keys: our own design, as `[xcpd]` names them.

use std::path::{Path, PathBuf};

use ferrofed_registry::secret::Secret;

use crate::config::error::Error;
use crate::config::secrets::secret;

/// The TLS material of one service, with every file read.
///
/// `Debug` shows the client identity as `***`, because [`Secret`] does.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TlsSettings {
    /// The gateway's client certificate chain and private key, PEM.
    pub client_identity: Option<Secret>,
    /// The PEM trust roots beside the platform's.
    pub trust_roots: Option<String>,
}

/// Reads the TLS material of the table at `key`: the client identity, inline
/// or from `identity_file`, and the trust roots from `roots_file`.
///
/// # Errors
///
/// [`Error::Conflict`] for an identity given inline and as a file,
/// [`Error::Secret`] and [`Error::EmptySecret`] for a file that cannot be
/// read or is empty.
pub(crate) fn resolve(
    key: &str,
    identity: Option<&Secret>,
    identity_file: Option<&Path>,
    roots_file: Option<&PathBuf>,
) -> Result<TlsSettings, Error> {
    let client_identity = secret(&format!("{key}.client_identity"), identity, identity_file)?;
    let trust_roots = roots_file
        .map(|path| {
            std::fs::read_to_string(path).map_err(|source| Error::Secret {
                key: format!("{key}.trust_roots_file"),
                path: path.clone(),
                source,
            })
        })
        .transpose()?;
    Ok(TlsSettings {
        client_identity,
        trust_roots,
    })
}
