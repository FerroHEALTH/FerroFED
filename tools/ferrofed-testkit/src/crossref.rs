// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The fixed cross-reference the offline suites resolve patients through.
//!
//! FerroFED keeps no patient index: a deployment resolves patients through a
//! PIX Manager. A test still needs a fixed answer, so it writes the rows it
//! needs as `[[dev.crossref]]` (and `[[dev.consent_denied]]`) tables in the
//! configuration text it builds, and [`inject_from`] takes them out of the
//! text and injects the [`StaticResolver`] and [`StaticConsentPrefilter`]
//! they describe through the server's `test-seam` injection point. A
//! production build has no such point. No specification governs the
//! fixture: our own design.
//!
//! As a resolver the fixture answers `Resolved` for a row of the table and
//! `Unknown` for every other member asked; as a localizer it names the
//! members that have a row for the patient, and answers `NoRecords` when none
//! has. The consent pre-filter denies the members a row names for the
//! patient.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use ferrofed_identity::role::behalf::OnBehalfOf;
use ferrofed_identity::role::consent::{ConsentDecision, ConsentPrefilter, Requester};
use ferrofed_identity::role::localizer::{Localization, Localizer};
use ferrofed_identity::role::patient::{IdentifierNamespace, PatientRef};
use ferrofed_identity::role::resolver::{Identification, Resolution, Resolver};
use ferrofed_registry::id::{EhrId, NodeId};
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_server::binding::injected::{Injection, PrefilterFactory, inject};
use ferrofed_server::binding::seam::ResolverSeam;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

/// The mode `OPTIONS {base}/` declares the fixture's localizer and consent
/// pre-filter under.
pub const MODE: &str = "development-static";

/// One cross-reference row, as a test writes it.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryDoc {
    namespace: IdentifierNamespace,
    value: String,
    member: NodeId,
    ehr_id: EhrId,
}

/// One consent row, as a test writes it: the patient whose consent denies
/// asking `member`.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConsentDoc {
    namespace: IdentifierNamespace,
    value: String,
    member: NodeId,
}

/// The `[dev]` rows of a test's configuration text.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Rows {
    #[serde(default)]
    crossref: Vec<EntryDoc>,
    #[serde(default)]
    consent_denied: Vec<ConsentDoc>,
}

/// Takes the `[dev]` table out of the configuration `text` and injects the
/// fixture its rows describe, or clears the injection when `text` has none.
///
/// Returns the text without the table. A text that is no TOML is returned as
/// it is, with the injection cleared, so a test of a refused configuration
/// reads its own error.
///
/// # Panics
///
/// Never: a table read from TOML text always writes back as TOML.
#[must_use]
#[expect(
    clippy::expect_used,
    reason = "a toml::Table parsed from text always serialises back to text"
)]
pub fn inject_from(text: &str) -> String {
    let Ok(mut table) = text.parse::<toml::Table>() else {
        inject(None);
        return text.to_owned();
    };
    let Some(dev) = table.remove("dev") else {
        inject(None);
        return text.to_owned();
    };
    let Ok(rows) = dev.try_into::<Rows>() else {
        inject(None);
        return text.to_owned();
    };
    let crossref = rows.crossref;
    let consent = rows.consent_denied;
    let prefilter = (!consent.is_empty()).then(|| {
        let factory: PrefilterFactory = Arc::new(move |snapshot: &RegistrySnapshot| {
            StaticConsentPrefilter::new(&consent, snapshot)
                .map(|prefilter| -> Arc<dyn ConsentPrefilter> { Arc::new(prefilter) })
        });
        factory
    });
    inject(Some(Injection {
        resolver: Arc::new(move |snapshot: &RegistrySnapshot| {
            let resolver = Arc::new(StaticResolver::new(&crossref, snapshot)?);
            let localizer: Arc<dyn Localizer> = resolver.clone();
            Ok(ResolverSeam {
                resolver,
                localizer: Some((localizer, MODE)),
            })
        }),
        prefilter,
    }));
    toml::to_string(&table).expect("a parsed table should write back as TOML")
}

/// One row of the fixture.
struct Row {
    namespace: IdentifierNamespace,
    value: SecretString,
    member: NodeId,
    ehr_id: EhrId,
}

/// The [`Resolver`] and the [`Localizer`] over the fixed cross-reference.
pub struct StaticResolver {
    rows: Vec<Row>,
}

impl StaticResolver {
    /// The resolver over `entries`, each naming a member of `registry`.
    fn new(entries: &[EntryDoc], registry: &RegistrySnapshot) -> Result<Self, String> {
        let mut rows: Vec<Row> = Vec::with_capacity(entries.len());
        for entry in entries {
            if entry.value.is_empty() {
                return Err(format!(
                    "a cross-reference row in namespace {} has an empty value",
                    entry.namespace
                ));
            }
            if registry.node(&entry.member).is_none() {
                return Err(format!(
                    "a cross-reference row names member {}, which is not in the registry",
                    entry.member
                ));
            }
            if rows.iter().any(|row| {
                row.namespace == entry.namespace
                    && row.member == entry.member
                    && row.value.expose_secret() == entry.value
            }) {
                return Err(format!(
                    "two cross-reference rows map one identifier in namespace {} at member {}",
                    entry.namespace, entry.member
                ));
            }
            rows.push(Row {
                namespace: entry.namespace.clone(),
                value: entry.value.clone().into(),
                member: entry.member.clone(),
                ehr_id: entry.ehr_id.clone(),
            });
        }
        Ok(Self { rows })
    }

    fn lookup(&self, patient: &PatientRef, member: &NodeId) -> Option<&EhrId> {
        self.rows
            .iter()
            .find(|row| {
                row.member == *member
                    && row.namespace == *patient.namespace()
                    && row.value.expose_secret() == patient.withheld().expose_secret()
            })
            .map(|row| &row.ehr_id)
    }
}

impl fmt::Debug for StaticResolver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StaticResolver")
            .field("rows", &self.rows.len())
            .finish()
    }
}

#[async_trait]
impl Resolver for StaticResolver {
    async fn resolve(
        &self,
        patient: &PatientRef,
        members: &[NodeId],
        _on_behalf: &OnBehalfOf,
        _deadline: Instant,
    ) -> BTreeMap<NodeId, Resolution> {
        members
            .iter()
            .map(|member| {
                let resolution = match self.lookup(patient, member) {
                    Some(ehr_id) => Resolution::Resolved(ehr_id.clone()),
                    None => Resolution::Unknown,
                };
                (member.clone(), resolution)
            })
            .collect()
    }

    async fn identify(
        &self,
        member: &NodeId,
        ehr_id: &EhrId,
        namespaces: &[IdentifierNamespace],
        _on_behalf: &OnBehalfOf,
        _deadline: Instant,
    ) -> Identification {
        let named: Vec<PatientRef> = self
            .rows
            .iter()
            .filter(|row| {
                row.member == *member
                    && row.ehr_id == *ehr_id
                    && namespaces.contains(&row.namespace)
            })
            // NOTE: no specification governs this: our own design; a row with an empty
            // value is refused when the fixture is built, so no row is skipped here.
            .filter_map(|row| PatientRef::new(row.namespace.clone(), row.value.clone()).ok())
            .collect();
        if named.is_empty() {
            Identification::Unknown
        } else {
            Identification::Named(named)
        }
    }
}

#[async_trait]
impl Localizer for StaticResolver {
    async fn localize(
        &self,
        patient: &PatientRef,
        members: &[NodeId],
        _on_behalf: &OnBehalfOf,
        _deadline: Instant,
    ) -> Localization {
        let candidates: BTreeSet<NodeId> = members
            .iter()
            .filter(|member| self.lookup(patient, member).is_some())
            .cloned()
            .collect();
        if candidates.is_empty() {
            Localization::NoRecords
        } else {
            Localization::Candidates(candidates)
        }
    }
}

/// One patient whose consent denies asking one member.
struct Denial {
    namespace: IdentifierNamespace,
    value: SecretString,
    member: NodeId,
}

/// The [`ConsentPrefilter`] over the fixture's consent rows (N27a).
pub struct StaticConsentPrefilter {
    denials: Vec<Denial>,
}

impl StaticConsentPrefilter {
    /// The pre-filter over `rows`, each naming a member of `registry`.
    fn new(rows: &[ConsentDoc], registry: &RegistrySnapshot) -> Result<Self, String> {
        let mut denials: Vec<Denial> = Vec::with_capacity(rows.len());
        for row in rows {
            if row.value.is_empty() {
                return Err(format!(
                    "a consent row in namespace {} has an empty value",
                    row.namespace
                ));
            }
            if registry.node(&row.member).is_none() {
                return Err(format!(
                    "a consent row names member {}, which is not in the registry",
                    row.member
                ));
            }
            if denials.iter().any(|denial| {
                denial.namespace == row.namespace
                    && denial.member == row.member
                    && denial.value.expose_secret() == row.value
            }) {
                return Err(format!(
                    "two consent rows deny one identifier in namespace {} at member {}",
                    row.namespace, row.member
                ));
            }
            denials.push(Denial {
                namespace: row.namespace.clone(),
                value: row.value.clone().into(),
                member: row.member.clone(),
            });
        }
        Ok(Self { denials })
    }

    fn denies(&self, patient: &PatientRef, member: &NodeId) -> bool {
        self.denials.iter().any(|denial| {
            denial.member == *member
                && denial.namespace == *patient.namespace()
                && denial.value.expose_secret() == patient.withheld().expose_secret()
        })
    }
}

impl fmt::Debug for StaticConsentPrefilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StaticConsentPrefilter")
            .field("rows", &self.denials.len())
            .finish()
    }
}

#[async_trait]
impl ConsentPrefilter for StaticConsentPrefilter {
    async fn prefilter(
        &self,
        patient: &PatientRef,
        _requester: Option<&Requester>,
        candidates: &[NodeId],
        _deadline: Instant,
    ) -> ConsentDecision {
        let denied: BTreeSet<NodeId> = candidates
            .iter()
            .filter(|member| self.denies(patient, member))
            .cloned()
            .collect();
        if denied.is_empty() {
            ConsentDecision::NoSignal
        } else {
            ConsentDecision::Denied(denied)
        }
    }

    fn mode(&self) -> &'static str {
        MODE
    }

    fn budget(&self) -> Option<Duration> {
        None
    }
}
