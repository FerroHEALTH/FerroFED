// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `OPTIONS {base}/` body: the two schema conditionals, the closed
//! defaults, and the guards the schema states with `not` (§7a.2, N30, CP-23).

use openehr_federation::error::WireError;
use openehr_federation::id::{EndpointId, FederationId};
use openehr_federation::object::Extra;
use openehr_federation::options::{
    Aggregates, AqlBehaviour, Completeness, DedupDefault, DedupModes, DedupPolicy,
    DefinitionBehaviour, DemographicSupport, GatewayDescription, ItsRestAreas, Localization,
    MemberEndpoint, MembershipStatus, OptIn, OptionsRoot, Paging, SpecVersion, TimeoutPolicy,
};

use crate::support;

fn options_example() -> String {
    support::only_example("rest-facade.adoc").expect("§7a.2 carries one example")
}

fn refused(text: &str) -> String {
    serde_json::from_str::<OptionsRoot>(text)
        .expect_err("the body should have been refused")
        .to_string()
}

/// A minimal conformant body built only through the constructors.
fn built() -> OptionsRoot {
    OptionsRoot {
        federation: GatewayDescription {
            id: FederationId::new("example-federation").expect("non-empty"),
            spec_version: SpecVersion::of_release(openehr_federation::FEDERATION_SPEC)
                .expect("a release"),
            aql: AqlBehaviour {
                fan_out: true,
                extra: Extra::new(),
            },
            dedup: DedupPolicy {
                default: DedupDefault,
                modes: DedupModes::new(vec!["none".to_owned()]).expect("one mode"),
                request_header: None,
                extra: Extra::new(),
            },
            timeout: TimeoutPolicy {
                per_node_ms: 5000,
                overall_ms: 15000,
                policy: "all-or-nothing".to_owned(),
                extra: Extra::new(),
            },
            completeness: Completeness::with_best_effort(OptIn {
                header: Some(openehr_federation::headers::COMPLETENESS.to_owned()),
                value: Some(openehr_federation::headers::COMPLETENESS_PARTIAL.to_owned()),
                extra: Extra::new(),
            }),
            paging: Paging {
                offset_strategy: "reject".to_owned(),
                extra: Extra::new(),
            },
            aggregates: Aggregates {
                decomposable: Vec::new(),
                extra: Extra::new(),
            },
            definition: DefinitionBehaviour::new(false)
                .with_stored_query_registry(true)
                .and_then(|definition| definition.with_stored_query_fan_out(false))
                .expect("registry without fan-out"),
            localization: Localization {
                on_failure: "closed".to_owned(),
                extra: Extra::new(),
            },
            auth: None,
            its_rest: ItsRestAreas {
                query: "federated".to_owned(),
                ehr: "routed".to_owned(),
                definition: "routed".to_owned(),
                demographic: DemographicSupport::new("unsupported").expect("not federated"),
                extra: Extra::new(),
            },
            extra: Extra::new(),
        },
        endpoints: vec![MemberEndpoint {
            id: EndpointId::new("node_1").expect("non-empty"),
            organisation: "Org A".to_owned(),
            status: MembershipStatus::new("active").expect("a membership status"),
            node_id: None,
            system_id: None,
            product: None,
            version: None,
            latency_ms_p50: None,
            url: None,
            extra: Extra::new(),
        }],
        extra: Extra::new(),
    }
}

#[test]
fn a_body_built_from_the_constructors_validates() {
    support::validate(support::OPTIONS_SCHEMA, &built()).expect("the built body validates");
    let text = serde_json::to_string(&built()).expect("the body serializes");
    let again: OptionsRoot = serde_json::from_str(&text).expect("the body reads");
    assert_eq!(again, built(), "the body round-trips");
}

#[test]
fn spec_version_is_major_minor() {
    assert_eq!(
        SpecVersion::of_release("0.9.0")
            .expect("a release")
            .as_str(),
        "0.9",
        "§7a.2: 0.9.0 reports 0.9"
    );
    assert!(SpecVersion::new("0.9").is_ok(), "major.minor");
    for refused in ["0.9.0", "0", "0.", ".9", "v0.9", "0.9-rc", ""] {
        assert!(
            matches!(SpecVersion::new(refused), Err(WireError::SpecVersion)),
            "`{refused}` is not major.minor"
        );
    }
    let text =
        support::with(&options_example(), "/federation/spec_version", r#""0.9.0""#).expect("edit");
    assert!(
        refused(&text).contains("spec_version"),
        "the reader refuses a patch component"
    );
}

#[test]
fn best_effort_requires_opt_in() {
    let text =
        support::without(&options_example(), "/federation/completeness/opt_in").expect("edit");
    assert!(refused(&text).contains("opt_in"), "N37, §11.4");
    let declined =
        support::with(&text, "/federation/completeness/best_effort", "false").expect("edit");
    serde_json::from_str::<OptionsRoot>(&declined)
        .expect("no opt_in is needed when best-effort is not offered");
}

#[test]
fn the_completeness_default_is_all_or_nothing() {
    let text = support::with(
        &options_example(),
        "/federation/completeness/default",
        r#""best-effort""#,
    )
    .expect("edit");
    assert!(
        refused(&text).contains("all-or-nothing"),
        "N37: the only conformant default"
    );
}

#[test]
fn the_dedup_default_is_none_and_modes_are_listed() {
    let text = support::with(
        &options_example(),
        "/federation/dedup/default",
        r#""version-identity""#,
    )
    .expect("edit");
    assert!(
        refused(&text).contains("none"),
        "N15: the default MUST be none"
    );
    let text = support::with(&options_example(), "/federation/dedup/modes", "[]").expect("edit");
    assert!(refused(&text).contains("modes"), "minItems 1");
    assert!(
        matches!(DedupModes::new(Vec::new()), Err(WireError::NoDedupModes)),
        "the constructor refuses an empty list"
    );
}

#[test]
fn stored_query_fan_out_requires_the_registry() {
    let fan_out = support::with(
        &options_example(),
        "/federation/definition/stored_query_fan_out",
        "true",
    )
    .expect("edit");
    serde_json::from_str::<OptionsRoot>(&fan_out)
        .expect("fan-out with the registry offered is allowed");
    let without_registry =
        support::without(&fan_out, "/federation/definition/stored_query_registry").expect("edit");
    assert!(
        refused(&without_registry).contains("stored_query_registry"),
        "§12.7, N44"
    );
    let registry_off = support::with(
        &fan_out,
        "/federation/definition/stored_query_registry",
        "false",
    )
    .expect("edit");
    assert!(
        refused(&registry_off).contains("stored_query_registry"),
        "§12.7, N44"
    );
    assert!(
        matches!(
            DefinitionBehaviour::new(false).with_stored_query_fan_out(true),
            Err(WireError::FanOutWithoutRegistry)
        ),
        "the builder refuses it too"
    );
}

#[test]
fn the_demographic_area_is_never_federated() {
    let text = support::with(
        &options_example(),
        "/federation/its_rest/demographic",
        r#""federated""#,
    )
    .expect("edit");
    assert!(refused(&text).contains("demographic"), "N32");
}

#[test]
fn not_localized_is_never_a_membership_status() {
    let text = support::with(
        &options_example(),
        "/endpoints/0/status",
        r#""not-localized""#,
    )
    .expect("edit");
    assert!(
        refused(&text).contains("not-localized"),
        "§7a.2: the two vocabularies stay apart"
    );
    assert!(
        MembershipStatus::new("draining").is_ok(),
        "§7a.2: the membership vocabulary is open"
    );
}

#[test]
fn auth_is_optional_and_a_jwks_uri_must_be_a_uri() {
    let text = support::without(&options_example(), "/federation/auth").expect("edit");
    serde_json::from_str::<OptionsRoot>(&text).expect("§13.1: a gateway with no JWKS omits auth");
    let text = support::with(
        &options_example(),
        "/federation/auth/jwks_uri",
        r#""not a uri""#,
    )
    .expect("edit");
    assert!(refused(&text).contains("jwks_uri"), "format: uri");
}
