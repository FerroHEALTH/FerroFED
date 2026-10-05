// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Each live [`Scenario`] run against the deployment with what the fixture
//! holds, to one [`Outcome`].
//!
//! A scenario whose precondition the deployment does not meet (fewer
//! members holding the patient than it compares, no member where the
//! patient does not resolve, no stored-query registry declared) is
//! `not-run` with the reason, never `pass`. Every reason and every failure
//! is redacted against the run's patient before it is kept.

use std::fmt;

use crate::conformance::Failure;
use crate::conformance::catalogue::{Entry, Kind, Scenario};
use crate::conformance::client::Gateway;
use crate::conformance::fixture::{Fixture, Member, SeedData, composition_carrying};
use crate::conformance::scenarios::{
    track1, track2, track3, track4, track5, track6, track7, track9, track11,
};
use crate::conformance::seed::Written;

/// The version every stored query of a run is stored at.
pub const STORED_VERSION: &str = "1.0.0";

/// The namespace every stored query of a run is stored under.
pub const STORED_NAMESPACE: &str = "org.example.conformance";

/// What one catalogue entry came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Every check held.
    Pass,
    /// A check did not hold; the text says which.
    Fail(String),
    /// The scenario did not run; the text says why.
    NotRun(String),
}

impl Outcome {
    /// The result word of the report: `pass`, `fail` or `not-run`.
    #[must_use]
    pub fn word(&self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail(_) => "fail",
            Self::NotRun(_) => "not-run",
        }
    }

    /// Why the entry did not pass, when it did not.
    #[must_use]
    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Pass => None,
            Self::Fail(reason) | Self::NotRun(reason) => Some(reason),
        }
    }
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.reason() {
            Some(reason) => write!(f, "{}: {reason}", self.word()),
            None => f.write_str(self.word()),
        }
    }
}

/// What the scenarios of one run read besides the gateway.
#[derive(Debug)]
pub struct Context<'r> {
    /// The fixture.
    pub fixture: &'r Fixture,
    /// The vendored content the run commits.
    pub seed: &'r SeedData,
    /// The run's own token, which names its stored queries.
    pub run: &'r str,
}

/// The precondition of a scenario that the deployment does not meet.
#[derive(Debug)]
struct Unmet(String);

/// Runs `entry` against `gateway`, records in `written` every write it
/// makes, and returns its outcome.
pub async fn execute<G: Gateway>(
    gateway: &G,
    context: &Context<'_>,
    entry: &Entry,
    written: &mut Vec<Written>,
) -> Outcome {
    let outcome = match entry.kind {
        Kind::Harness(reason) => Outcome::NotRun(reason.to_owned()),
        Kind::Live(scenario) => match live(gateway, context, scenario, written).await {
            Ok(Ok(())) => Outcome::Pass,
            Ok(Err(failure)) => Outcome::Fail(crate::chain(&failure)),
            Err(Unmet(reason)) => Outcome::NotRun(reason),
        },
    };
    let redact = |text: &str| context.fixture.patient.redact(text);
    match outcome {
        Outcome::Pass => Outcome::Pass,
        Outcome::Fail(reason) => Outcome::Fail(redact(&reason)),
        Outcome::NotRun(reason) => Outcome::NotRun(redact(&reason)),
    }
}

/// The first `count` holding members, or the precondition unmet.
fn holding(fixture: &Fixture, count: usize) -> Result<Vec<&Member>, Unmet> {
    fixture
        .first_holding(count)
        .map(|members| members.into_iter().map(|(member, _)| member).collect())
        .map_err(Unmet)
}

/// The first holding member.
fn first(fixture: &Fixture) -> Result<&Member, Unmet> {
    holding(fixture, 1)?
        .into_iter()
        .next()
        .ok_or_else(|| Unmet("needs a member holding the patient".to_owned()))
}

/// The second holding member, else the first.
fn second_or_first(fixture: &Fixture) -> Result<&Member, Unmet> {
    let first = first(fixture)?;
    Ok(fixture.holding().nth(1).map_or(first, |(member, _)| member))
}

/// The `ehr_id` of `member`'s holding.
fn ehr_of(member: &Member) -> Result<(&str, usize), Unmet> {
    member
        .holding
        .as_ref()
        .map(|holding| (holding.ehr_id.as_str(), holding.compositions))
        .ok_or_else(|| Unmet(format!("{} holds the patient", member.endpoint)))
}

/// Whether the gateway declares its stored-query registry, or the unmet
/// precondition of the registry's clauses (§16.3 track 9: "where a gateway
/// declares `definition.stored_query_registry`").
async fn registry_declared<G: Gateway>(gateway: &G) -> Result<Result<(), Failure>, Unmet> {
    match track9::options(gateway).await {
        Ok(root) if root.federation.definition.stored_query_registry() == Some(true) => Ok(Ok(())),
        Ok(_) => Err(Unmet(
            "the gateway declares no definition.stored_query_registry, so the registry clauses of track 9 do not apply to it".to_owned(),
        )),
        Err(failure) => Ok(Err(failure)),
    }
}

/// Runs one live scenario, or reports its unmet precondition.
#[expect(
    clippy::too_many_lines,
    reason = "one arm per scenario keeps the catalogue order and its checks side by side"
)]
async fn live<G: Gateway>(
    gateway: &G,
    context: &Context<'_>,
    scenario: Scenario,
    written: &mut Vec<Written>,
) -> Result<Result<(), Failure>, Unmet> {
    let fixture = context.fixture;
    let stored = |query: &str| format!("{STORED_NAMESPACE}::{query}_{}", context.run);
    Ok(match scenario {
        Scenario::SingleCdrShaped => {
            holding(fixture, 2)?;
            track1::single_cdr_shaped(gateway, fixture).await.map(drop)
        }
        Scenario::BothCarriers => {
            holding(fixture, 2)?;
            track2::both_carriers(gateway, fixture).await.map(drop)
        }
        Scenario::SubjectColumn => {
            first(fixture)?;
            track2::subject_column(gateway, fixture).await
        }
        Scenario::DirectiveEndpoint => {
            let endpoint = &first(fixture)?.endpoint;
            track3::directive_endpoint(gateway, fixture, endpoint)
                .await
                .map(drop)
        }
        Scenario::EndpointAttributes => {
            first(fixture)?;
            let named: Vec<&str> = fixture
                .holding()
                .map(|(member, _)| member.endpoint.as_str())
                .collect();
            track3::endpoint_attributes(gateway, fixture, &named)
                .await
                .map(drop)
        }
        Scenario::DirectiveOrganisation => {
            let organisation = &second_or_first(fixture)?.organisation;
            track3::directive_organisation(gateway, fixture, organisation)
                .await
                .map(drop)
        }
        Scenario::NamedUnresolved => {
            let holding = &first(fixture)?.endpoint;
            let unresolved = fixture
                .not_holding()
                .find(|member| member.active)
                .ok_or_else(|| {
                    Unmet("needs an active member where the synthetic patient does not resolve, and the cross-reference resolves it at every one".to_owned())
                })?;
            track3::named_unresolved(gateway, fixture, holding, &unresolved.endpoint)
                .await
                .map(drop)
        }
        Scenario::EndpointHeader => {
            let endpoint = &second_or_first(fixture)?.endpoint;
            track3::header_selects(gateway, fixture, endpoint).await
        }
        Scenario::TargetingConflict => {
            let directed = &second_or_first(fixture)?.endpoint;
            let other = fixture
                .members
                .iter()
                .find(|member| member.endpoint != *directed)
                .ok_or_else(|| Unmet("needs a second member to name in the header".to_owned()))?;
            track3::conflict_refused(gateway, fixture, directed, &other.endpoint).await
        }
        Scenario::EndpointParameter => {
            let endpoint = &second_or_first(fixture)?.endpoint;
            track3::parameter_targets_nothing(gateway, fixture, endpoint).await
        }
        Scenario::FoundNowhere => {
            let unknown = fixture.patient.unknown_beside();
            track4::found_nowhere(gateway, fixture, &unknown)
                .await
                .map(drop)
        }
        Scenario::DuplicatesAndDistinct => {
            holding(fixture, 2)?;
            track5::duplicates_and_distinct(gateway, fixture).await
        }
        Scenario::OrderAndLimit => {
            holding(fixture, 2)?;
            track5::order_and_limit(gateway, fixture).await.map(drop)
        }
        Scenario::AggregateAsDeclared => {
            first(fixture)?;
            match track9::options(gateway).await {
                Ok(root)
                    if root
                        .federation
                        .aggregates
                        .decomposable
                        .iter()
                        .any(|name| name == "COUNT") =>
                {
                    track5::aggregate_recombined(gateway, fixture).await
                }
                Ok(_) => track5::aggregate_refused(gateway, fixture).await,
                Err(failure) => Err(failure),
            }
        }
        Scenario::AggregateDirected => {
            let endpoint = &first(fixture)?.endpoint;
            track5::aggregate_directed(gateway, fixture, endpoint).await
        }
        Scenario::FollowUpRead => {
            first(fixture)?;
            match track6::row_uids(gateway, fixture).await {
                Ok(uids) => {
                    let mut read = Ok(());
                    for uid in &uids {
                        read = track6::follow_up_read(gateway, fixture, uid)
                            .await
                            .map(drop);
                        if read.is_err() {
                            break;
                        }
                    }
                    read
                }
                Err(failure) => Err(failure),
            }
        }
        Scenario::ProbeRead => {
            let (endpoint, ehr_id) =
                fixture.spares.probe.as_ref().ok_or_else(|| {
                    Unmet(spare_missing(fixture, "an EHR the gateway has not seen"))
                })?;
            let system_id = fixture
                .member(endpoint)
                .map(|member| member.system_id.as_str())
                .ok_or_else(|| Unmet(format!("{endpoint} is a member")))?;
            track6::probe_read(gateway, endpoint, system_id, ehr_id).await
        }
        Scenario::UnroutedWrite => {
            let ehr_id =
                fixture.spares.unrouted.as_ref().ok_or_else(|| {
                    Unmet(spare_missing(fixture, "an EHR no step routes a write to"))
                })?;
            match track6::unrouted_write(gateway, ehr_id).await {
                Ok(()) => track6::write_nowhere(gateway).await,
                Err(failure) => Err(failure),
            }
        }
        Scenario::NewEhrUntargeted => track6::new_ehr_untargeted(gateway).await,
        Scenario::NewEhrNamed => {
            let member = second_or_first(fixture)?;
            let created = track6::new_ehr_at(gateway, member).await;
            written.push(Written {
                endpoint: member.endpoint.clone(),
                what: match &created {
                    Ok(location) => format!("EHR with no subject, created through the gateway at {location}"),
                    Err(_) => "EHR with no subject, asked of the gateway; whether the node created it is not known".to_owned(),
                },
            });
            created.map(drop)
        }
        Scenario::Unauthenticated => track7::unauthenticated(gateway, fixture).await,
        Scenario::SelfDescription => track9::self_description(gateway, fixture).await.map(drop),
        Scenario::DefinitionNamed => {
            let endpoint = second_or_first(fixture)
                .map(|member| member.endpoint.as_str())
                .or_else(|_| {
                    fixture
                        .members
                        .first()
                        .map(|member| member.endpoint.as_str())
                        .ok_or_else(|| Unmet("needs a member".to_owned()))
                })?;
            track9::definition_named(gateway, endpoint).await
        }
        Scenario::DefinitionUnnamed => match track9::definition_unnamed(gateway, None).await {
            Ok(()) => track9::definition_unnamed(gateway, Some("*")).await,
            Err(failure) => Err(failure),
        },
        Scenario::StoredQueryRun => {
            first(fixture)?;
            if let Err(failure) = registry_declared(gateway).await? {
                return Ok(Err(failure));
            }
            let name = stored("patient_compositions");
            written.push(stored_written(&name));
            track9::store_and_run(gateway, fixture, (&name, STORED_VERSION)).await
        }
        Scenario::StoredQuerySecondPut => {
            first(fixture)?;
            if let Err(failure) = registry_declared(gateway).await? {
                return Ok(Err(failure));
            }
            let name = stored("patient_compositions");
            track9::second_put_refused(gateway, fixture, (&name, STORED_VERSION)).await
        }
        Scenario::StoredQueryDirected => {
            let endpoint = &first(fixture)?.endpoint;
            if let Err(failure) = registry_declared(gateway).await? {
                return Ok(Err(failure));
            }
            let name = stored("directed_compositions");
            written.push(stored_written(&name));
            track9::directed_store_and_run(gateway, fixture, (&name, STORED_VERSION), endpoint)
                .await
        }
        Scenario::PlainClient => {
            let member = first(fixture)?;
            let ehr = ehr_of(member)?;
            match composition_carrying(context.seed.hospital(), &fixture.patient) {
                Ok(composition) => {
                    let wrote = track9::plain_client(gateway, member, ehr, &composition).await;
                    written.push(Written {
                        endpoint: member.endpoint.clone(),
                        what: match &wrote {
                            Ok(reply) => format!(
                                "composition {} in EHR {}, committed through the gateway, its composer carrying the synthetic patient's identifier",
                                reply.field("etag").unwrap_or("with no ETag"),
                                ehr.0
                            ),
                            Err(_) => format!(
                                "composition in EHR {}, asked of the gateway; whether the node committed it is not known",
                                ehr.0
                            ),
                        },
                    });
                    wrote.map(drop)
                }
                Err(failure) => Err(failure),
            }
        }
        Scenario::VersionedWriteUnrouted => {
            let (ehr_id, version) = fixture.spares.versioned.as_ref().ok_or_else(|| {
                Unmet(spare_missing(
                    fixture,
                    "an EHR with a version no step routes",
                ))
            })?;
            track11::versioned_writes_unrouted(gateway, ehr_id, version, context.seed.hospital())
                .await
                .map(drop)
        }
    })
}

/// Why the spare `what` is missing.
fn spare_missing(fixture: &Fixture, what: &str) -> String {
    match &fixture.shortfall {
        Some(shortfall) => {
            format!("needs {what}, which the run seeds only beside the patient: {shortfall}")
        }
        None => format!("needs {what}, which the run did not create"),
    }
}

/// The record of the stored query `name`.
fn stored_written(name: &str) -> Written {
    Written {
        endpoint: "the gateway".to_owned(),
        what: format!(
            "stored query {name} {STORED_VERSION} in the gateway's registry, its patient bound only as $patient"
        ),
    }
}
