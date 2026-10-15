// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[registry]` table: where the federation's membership is read from,
//! and the members the operator console manages beside it.
//!
//! The registry document, or the care services directory of the IHE
//! binding, is the file's: read-only in the console. Under
//! `config.ui = "managed"` a configuration revision may add
//! `[[registry.members]]`, each a whole member the console owns, joined to
//! the document's ([`RegistrySnapshot::with_console`]); the configuration
//! file never sets them, so nothing the console owns is written back to it.
//! No specification governs this: our own design.
//!
//! [`RegistrySnapshot::with_console`]: ferrofed_registry::snapshot::RegistrySnapshot::with_console

use std::path::PathBuf;

use ferrofed_registry::console::ConsoleMember;
use serde::Deserialize;

use crate::config::error::Error;
use crate::config::revisions::RevisionsFault;

/// The federation's membership.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Registry {
    /// The reviewed registry document naming the organisations, nodes and
    /// endpoints (§12b.1). Without it the gateway
    /// federates nothing, and the ITS-REST surface stays unserved.
    pub document: Option<PathBuf>,
    /// The form the document is written in.
    pub format: RegistryFormat,
    /// The mCSD care services directory the registry is read from and kept
    /// in step with (`[registry.mcsd]`, §15.1, Annex A.5), in place of a
    /// document.
    #[cfg(feature = "binding-ihe")]
    pub mcsd: Option<crate::binding::ihe::mcsd::McsdDirectory>,
    /// The members the operator console manages (`[[registry.members]]`),
    /// which only a configuration revision holds, joined to the document's.
    pub members: Vec<ConsoleMember>,
}

impl Registry {
    /// Whether a registry is configured, as a document or as a directory.
    #[must_use]
    pub fn configured(&self) -> bool {
        #[cfg(feature = "binding-ihe")]
        if self.mcsd.is_some() {
            return true;
        }
        self.document.is_some()
    }

    /// The members the operator console manages, when they can be joined.
    ///
    /// # Errors
    ///
    /// [`RevisionsFault::MembersWithoutDocument`] when members are set and
    /// no registry document is: a care services directory is the whole
    /// registry, and with no registry the gateway federates nothing.
    pub fn console_members(&self) -> Result<Vec<ConsoleMember>, Error> {
        if !self.members.is_empty() && self.document.is_none() {
            return Err(RevisionsFault::MembersWithoutDocument.into());
        }
        Ok(self.members.clone())
    }
}

/// The form of the registry document.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RegistryFormat {
    /// The native TOML form: `[[organisation]]`, `[[node]]`, `[[endpoint]]`
    /// and `[[creating_system]]` tables.
    #[default]
    Toml,
    /// A FHIR R4 JSON `Bundle` of `Organization` and `Endpoint` resources, the
    /// form N19 recommends, read with the IHE binding's mCSD reader.
    #[cfg(feature = "binding-ihe")]
    Fhir,
}
