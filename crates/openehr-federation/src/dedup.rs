// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The deduplication modes of §10 and the record of one applied
//! (`meta.federation.dedup`, §9.1, N15, N36).
//!
//! By default the Tier passes duplicates through (§10.1, N15). The opt-in
//! [`DedupMode::VersionIdentity`] keeps one copy of a version present at two
//! endpoints, the originating one (§10.2). A request selects a mode with the
//! request header `OPTIONS {base}/` names in `dedup.request_header` (§7a.2),
//! [`crate::headers::DEDUP`]; the mode names are the ones the §7a.2 example
//! declares. The rewrite of the `aql` feature and the merge of the `merge`
//! feature read the mode; it is plain data, so neither feature depends on the
//! other.
//!
//! # Examples
//!
//! ```
//! use openehr_federation::dedup::DedupMode;
//!
//! assert_eq!(DedupMode::default(), DedupMode::None);
//! assert_eq!(DedupMode::named("version-identity"), Some(DedupMode::VersionIdentity));
//! assert_eq!(DedupMode::named("content-hash"), None);
//! let record = DedupMode::None.record();
//! assert_eq!(record.mode.as_deref(), Some("none"));
//! ```

use crate::meta::DedupRecord;
use crate::object::Extra;

/// A deduplication mode (§10).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum DedupMode {
    /// Pass-through: every row every node returned, duplicates included
    /// (§10.1). The default, which N15 requires.
    #[default]
    None,
    /// One copy of every version present at more than one endpoint, the
    /// originating copy where there is one (§10.2, N15).
    VersionIdentity,
}

impl DedupMode {
    /// Every mode this crate implements, in declaration order: the list
    /// `OPTIONS {base}/` declares as `dedup.modes` (§7a.2).
    pub const OFFERED: [Self; 2] = [Self::None, Self::VersionIdentity];

    /// The name of the mode, as `meta.federation.dedup.mode`, the
    /// `dedup.modes` of `OPTIONS {base}/` and the request header carry it
    /// (the §7a.2 example: `none`, `version-identity`).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::VersionIdentity => "version-identity",
        }
    }

    /// The mode named `name`, or `None` for a name this crate does not
    /// implement. Names are matched exactly.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::OFFERED.into_iter().find(|mode| mode.name() == name)
    }

    /// The record of this mode with nothing suppressed: the `dedup` member of
    /// an answer with no rows to deduplicate, or of the pass-through mode
    /// (§10.2: "the mode actually applied MUST be recorded").
    #[must_use]
    pub fn record(self) -> DedupRecord {
        DedupRecord {
            mode: Some(self.name().to_owned()),
            suppressed_rows: None,
            suppressed_endpoints: None,
            extra: Extra::new(),
        }
    }
}
