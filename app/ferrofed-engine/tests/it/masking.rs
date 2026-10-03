// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The outbound gate masks the parts of a URL path the gateway composed from
//! trusted input, and only those (§5.4, N33): the endpoint's base path from
//! the registry, and the node's own `ehr_id` segment when the gateway wrote
//! it. A value inside them is chance, and passes; the same value anywhere the
//! client wrote is still found.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use ferrofed_engine::hygiene::{Composed, Outbound, Part, Withheld};
use secrecy::SecretString;
use url::{ParseError, Url};

type TestResult = Result<(), ParseError>;

/// The base path of an endpoint whose registry URL holds [`SHORT`].
const BASE_PATH: &str = "/cdr-4199/v1";

/// A node-local `ehr_id` that holds [`SHORT`].
const EHR_ID: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";

/// A short synthetic identifier.
const SHORT: &str = "4199";

fn short() -> Withheld {
    Withheld::new([SecretString::from(SHORT)])
}

/// The routed read of `target` under [`BASE_PATH`], with [`EHR_ID`] composed
/// by the gateway when `composed` is set.
fn routed<'a>(
    target: &'a Url,
    composed: bool,
    headers: &'a [(&'static str, &'a str)],
) -> Outbound<'a> {
    Outbound {
        aql: "",
        scope: None,
        paging: &[],
        url: target,
        composed: Composed {
            base_path: BASE_PATH,
            ehr_segment: composed.then_some(EHR_ID),
        },
        headers,
    }
}

/// The URL of the routed read of the EHR [`EHR_ID`].
fn ehr_url() -> Result<Url, ParseError> {
    Url::parse(&format!("https://cdr.example.org{BASE_PATH}/ehr/{EHR_ID}"))
}

// conformance: CP-26
#[test]
fn a_short_identifier_inside_the_composed_base_path_and_ehr_id_passes() -> TestResult {
    assert_eq!(None, short().found_in(&routed(&ehr_url()?, true, &[])));
    Ok(())
}

// conformance: CP-26
#[test]
fn the_same_short_identifier_in_a_client_part_is_still_found() -> TestResult {
    for text in [
        format!("https://cdr.example.org{BASE_PATH}/ehr/{EHR_ID}/composition/{SHORT}"),
        format!("https://cdr.example.org{BASE_PATH}/ehr/{EHR_ID}?version_at_time={SHORT}"),
        format!("https://cdr.example.org{BASE_PATH}/ehr/x{SHORT}/{EHR_ID}"),
    ] {
        let target = Url::parse(&text)?;
        assert_eq!(
            Some(Part::Url),
            short().found_in(&routed(&target, true, &[])),
            "{text}"
        );
    }
    let accept = format!("application/json; x={SHORT}");
    let headers = [("Accept", accept.as_str())];
    assert_eq!(
        Some(Part::Header("Accept")),
        short().found_in(&routed(&ehr_url()?, true, &headers))
    );
    Ok(())
}

// conformance: CP-26
#[test]
fn an_ehr_id_segment_the_client_wrote_is_never_masked() -> TestResult {
    assert_eq!(
        Some(Part::Url),
        short().found_in(&routed(&ehr_url()?, false, &[])),
        "only a segment the gateway composed is masked"
    );
    Ok(())
}

#[test]
fn a_base_path_is_masked_only_as_a_whole_leading_path() -> TestResult {
    let target = Url::parse(&format!("https://cdr.example.org{BASE_PATH}x/ehr/{EHR_ID}"))?;
    assert_eq!(
        Some(Part::Url),
        short().found_in(&routed(&target, true, &[]))
    );
    Ok(())
}

#[test]
fn an_identifier_holding_a_composed_part_fails_closed() -> TestResult {
    let target = ehr_url()?;
    for value in [EHR_ID, BASE_PATH] {
        let equal = Withheld::new([SecretString::from(value)]);
        assert_eq!(
            Some(Part::Url),
            equal.found_in(&routed(&target, true, &[])),
            "{value}"
        );
    }
    Ok(())
}
