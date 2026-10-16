// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `searchset` a prescription search answers: every member's
//! prescriptions, each as the member sent it, with one `Provenance` naming
//! the member it came from.
//!
//! Each prescription stays the member's own: the gateway adds no author, no
//! attester and no merged entry, and its prescriber stays its `requester`.
//! Its `Provenance` targets it and names the member's `Organization` as the
//! agent, both `include` entries (FHIR R4 Bundle `entry.search.mode`). A
//! prescription whose identifier a second member also holds is listed once
//! per member, and an `outcome` entry of severity `warning` says so; under
//! `partial`, each member that did not answer is named in an `outcome` entry
//! too (Federation Tier §11.3, §11.4). FHIR R4 `http.html` §3.1.0.9 allows
//! no `fatal` or `error` issue in a `searchset`. The `Provenance` and the
//! warning are our own design: no specification governs them.
//!
//! The `searchset` is the generated `fhir_types` `Bundle`. A member's
//! resource travels in it as `Resource::Unknown`, its JSON value as the
//! member sent it, every number in the member's own text; only the order of
//! an object's members may change, which RFC 8259 §4 gives no meaning.

use std::collections::{BTreeMap, BTreeSet};

use fhir_types::r4::bundle::{Bundle, BundleEntry, BundleEntrySearch, BundleLink};
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::operation_outcome::{OperationOutcome, OperationOutcomeIssue};
use fhir_types::r4::organization::Organization;
use fhir_types::r4::provenance::{Provenance, ProvenanceAgent};
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;
use url::Url;

use super::page::{Kept, Page, Prescription};
use super::{RESOURCE, SearchRequest};
use crate::summary::Organisation;

/// One member that answered the search, with every page it answered.
#[derive(Debug, Clone, Copy)]
pub struct Answered<'a> {
    /// The member's own endpoint.
    pub endpoint: &'a str,
    /// The organisation that holds the member's data.
    pub organisation: &'a Organisation,
    /// Its pages, in order.
    pub pages: &'a [Page],
}

/// One member that did not answer, under `partial`: its own endpoint and
/// its §11.1 status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Silent {
    /// The member's own endpoint.
    pub endpoint: String,
    /// Its §11.1 status.
    pub status: String,
}

/// What one page of the answer holds.
#[derive(Debug)]
pub struct Assembled {
    /// The page.
    pub searchset: Bundle,
    /// How many prescriptions of each member the page carries, by the
    /// member's position among those that answered.
    pub delivered: BTreeMap<usize, usize>,
    /// How many prescriptions each member answered with over every page, by
    /// the same position.
    pub held: BTreeMap<usize, usize>,
}

impl Assembled {
    /// Returns the number of prescriptions the page carries.
    #[must_use]
    pub fn carried(&self) -> usize {
        self.delivered.values().sum()
    }
}

/// Writes the page `request` asks for of the prescriptions `answered` holds.
///
/// The links are under `base`, the face's absolute base; every member of
/// `silent` is named; each `Provenance` is recorded at `recorded`, a FHIR
/// `instant`; and every `fullUrl` the gateway gives is taken from `mint`.
#[must_use]
pub fn assemble(
    (request, base): (&SearchRequest, &str),
    (answered, silent): (&[Answered<'_>], &[Silent]),
    recorded: &str,
    mint: &mut dyn FnMut() -> String,
) -> Assembled {
    let all: Vec<(usize, &Prescription)> = answered
        .iter()
        .enumerate()
        .flat_map(|(index, member)| {
            member
                .pages
                .iter()
                .flat_map(|page| page.prescriptions.iter())
                .map(move |prescription| (index, prescription))
        })
        .collect();
    let mut held = BTreeMap::new();
    for (index, _) in &all {
        *held.entry(*index).or_insert(0_usize) += 1;
    }
    let total = all.len();
    let offset = request.offset();
    let shown = request
        .count()
        .map_or(total, |count| count.min(total.saturating_sub(offset)));
    let mut writer = Writer {
        entries: Vec::new(),
        used: BTreeSet::new(),
        mint,
    };
    let mut delivered = BTreeMap::new();
    let mut matched = Vec::with_capacity(shown);
    for (index, prescription) in all.iter().skip(offset).take(shown) {
        let url = writer.push(&prescription.kept, "match");
        *delivered.entry(*index).or_insert(0_usize) += 1;
        matched.push((*index, url, *prescription));
    }
    let mut organisations = BTreeMap::new();
    for index in delivered.keys() {
        if let Some(member) = answered.get(*index) {
            let url = writer.own(organization(member.organisation), "include");
            organisations.insert(*index, url);
        }
    }
    for (index, url, _) in &matched {
        if let Some(agent) = organisations.get(index) {
            writer.own(provenance(url, agent, recorded), "include");
        }
    }
    for index in delivered.keys() {
        let carried = answered
            .get(*index)
            .into_iter()
            .flat_map(|member| member.pages.iter())
            .flat_map(|page| page.included.iter());
        for kept in carried {
            writer.push(kept, "include");
        }
    }
    for member in silent {
        writer.own(warning("incomplete", &unanswered(member)), "outcome");
    }
    for text in duplicates(answered, &all, &matched) {
        writer.own(warning("informational", &text), "outcome");
    }
    let outcomes = answered
        .iter()
        .flat_map(|member| member.pages.iter())
        .flat_map(|page| page.outcomes.iter());
    for kept in outcomes {
        writer.push(kept, "outcome");
    }
    let link = |relation: &str, offset: usize| BundleLink {
        relation: relation.into(),
        url: format!("{base}/{RESOURCE}?{}", request.page_query(offset)).into(),
        ..BundleLink::default()
    };
    let mut links = vec![link("self", offset)];
    let following = offset.saturating_add(shown);
    if request.count().is_some_and(|count| count > 0) && following < total {
        links.push(link("next", following));
    }
    Assembled {
        searchset: Bundle {
            r#type: "searchset".into(),
            total: u32::try_from(total).ok().map(Into::into),
            link: links,
            entry: writer.entries,
            ..Bundle::default()
        },
        delivered,
        held,
    }
}

/// The entries of a page as they are written, and every `fullUrl` they
/// already use.
struct Writer<'m> {
    entries: Vec<BundleEntry>,
    used: BTreeSet<String>,
    mint: &'m mut dyn FnMut() -> String,
}

impl Writer<'_> {
    /// Adds a member's resource `kept` as an entry of the search mode
    /// `mode`, and returns its `fullUrl`.
    ///
    /// The member's `fullUrl` is kept when it is an absolute URL no other
    /// entry uses; otherwise the entry is named by a minted one.
    fn push(&mut self, kept: &Kept, mode: &str) -> String {
        let url = kept
            .full_url
            .as_deref()
            .filter(|url| Url::parse(url).is_ok() && !self.used.contains(*url))
            .map_or_else(|| (self.mint)(), str::to_owned);
        self.add(url.clone(), kept.unknown(), mode);
        url
    }

    /// Adds the gateway's own `resource` as an entry of the search mode
    /// `mode`, under a minted `fullUrl`, and returns that `fullUrl`.
    fn own(&mut self, resource: Resource, mode: &str) -> String {
        let url = (self.mint)();
        self.add(url.clone(), resource, mode);
        url
    }

    /// Adds the entry `resource` under `url`, of the search mode `mode`.
    fn add(&mut self, url: String, resource: Resource, mode: &str) {
        self.used.insert(url.clone());
        self.entries.push(BundleEntry {
            full_url: Some(url.into()),
            resource: Some(resource),
            search: Some(BundleEntrySearch {
                mode: Some(mode.into()),
                ..BundleEntrySearch::default()
            }),
            ..BundleEntry::default()
        });
    }
}

/// The `Organization` of `organisation`.
fn organization(organisation: &Organisation) -> Resource {
    Resource::Organization(Box::new(Organization {
        name: Some(
            organisation
                .name
                .as_deref()
                .unwrap_or(&organisation.id)
                .into(),
        ),
        identifier: organisation
            .identifiers
            .iter()
            .map(|(system, value)| Identifier {
                system: Some(system.as_str().into()),
                value: Some(value.as_str().into()),
                ..Identifier::default()
            })
            .collect(),
        ..Organization::default()
    }))
}

/// The `Provenance` of the prescription at `target`, whose member's
/// `Organization` is at `agent`, recorded at `recorded`.
fn provenance(target: &str, agent: &str, recorded: &str) -> Resource {
    let reference = |url: &str| Reference {
        reference: Some(url.into()),
        ..Reference::default()
    };
    Resource::Provenance(Box::new(Provenance {
        target: vec![reference(target)],
        recorded: recorded.into(),
        agent: vec![ProvenanceAgent {
            who: reference(agent),
            ..ProvenanceAgent::default()
        }],
        ..Provenance::default()
    }))
}

/// An `OperationOutcome` of one `warning` of the issue type `code`.
fn warning(code: &str, diagnostics: &str) -> Resource {
    Resource::OperationOutcome(Box::new(OperationOutcome {
        issue: vec![OperationOutcomeIssue {
            severity: "warning".into(),
            code: code.into(),
            diagnostics: Some(diagnostics.into()),
            ..OperationOutcomeIssue::default()
        }],
        ..OperationOutcome::default()
    }))
}

/// The sentence that names `member`, which did not answer.
fn unanswered(member: &Silent) -> String {
    format!(
        "the search is incomplete: no answer from {} ({}) (§11.3)",
        member.endpoint, member.status
    )
}

/// One sentence for each identifier that more than one member of
/// `answered` holds a prescription under, among `all`, each prescription by
/// its member's position, when a prescription of the page, `matched`, each
/// also by its `fullUrl`, carries it.
///
/// The sentence names the members and the page's entries, never the
/// identifier.
fn duplicates(
    answered: &[Answered<'_>],
    all: &[(usize, &Prescription)],
    matched: &[(usize, String, &Prescription)],
) -> Vec<String> {
    let mut holders: BTreeMap<(&str, &str), BTreeSet<usize>> = BTreeMap::new();
    for (index, prescription) in all {
        for (system, value) in &prescription.identifiers {
            holders
                .entry((system.as_str(), value.as_str()))
                .or_default()
                .insert(*index);
        }
    }
    let mut on_page: BTreeMap<(&str, &str), Vec<&str>> = BTreeMap::new();
    for (_, url, prescription) in matched {
        for (system, value) in &prescription.identifiers {
            on_page
                .entry((system.as_str(), value.as_str()))
                .or_default()
                .push(url.as_str());
        }
    }
    holders
        .into_iter()
        .filter(|(_, members)| members.len() > 1)
        .filter_map(|(identifier, members)| {
            let entries = on_page.get(&identifier)?;
            let endpoints: Vec<&str> = members
                .iter()
                .filter_map(|index| answered.get(*index).map(|member| member.endpoint))
                .collect();
            Some(format!(
                "{} carries a prescription identifier that more than one member holds ({}); each member's prescription is listed as that member states it",
                entries.join(", "),
                endpoints.join(", ")
            ))
        })
        .collect()
}
