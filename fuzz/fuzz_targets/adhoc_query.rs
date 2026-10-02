// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Arbitrary bytes as the body of `POST {base}/v1/query/aql`, through the path
//! the façade runs: the ITS-REST `AdhocQueryExecute` decode, the
//! `query_parameters` intake, and the rewrite of §7.1. A decode error, an
//! intake error and a refusal are each the correct answer; a panic is the bug.
//!
//! The body is the input up to the first NUL byte. The bytes after it choose
//! the `OFFSET` strategy: an odd first byte refuses every `OFFSET k > 0`, and
//! anything else bounds `k + n` by a window drawn from the next bytes. With no
//! bytes after it, the strategy is the server default: `bounded`, 1000 rows.

#![no_main]

use std::num::NonZeroU32;

use ferrofed_server::facade::intake;
use libfuzzer_sys::arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;
use openehr_federation::aql::{Context, OffsetStrategy, Paging, Targeting, analyse};
use openehr_its::rest::generated::query::AdhocQueryExecute;

/// The rows the bounded strategy asks of one node when the input draws no
/// window: the server's default `federation.max_offset_window`.
const WINDOW: NonZeroU32 = NonZeroU32::MIN.saturating_add(999);

fuzz_target!(|data: &[u8]| {
    let (body, tail) = match data.iter().position(|byte| *byte == 0) {
        Some(nul) => (
            data.get(..nul).unwrap_or_default(),
            data.get(nul + 1..).unwrap_or_default(),
        ),
        None => (data, &[][..]),
    };
    let Ok(request) = serde_json::from_slice::<AdhocQueryExecute>(body) else {
        return;
    };
    let Ok(parameters) = intake::parameters(request.query_parameters.as_ref()) else {
        return;
    };
    let paging = Paging {
        offset: request.offset,
        fetch: request.fetch,
    };
    let context = Context::new(Targeting::AskAll)
        .with_default_namespace("urn:oid:2.999.1")
        .with_offset_strategy(offset_strategy(&mut Unstructured::new(tail)));
    let _analysis = analyse(&request.q, &parameters, paging, &context);
});

/// How `OFFSET k > 0` is answered (§11.6.2, N39), chosen by the input.
fn offset_strategy(choices: &mut Unstructured<'_>) -> OffsetStrategy {
    // NOTE: no specification governs this: our own design; an exhausted input
    // reads `false` and a zero window, so a bare body runs the server default.
    let reject: bool = choices.arbitrary().unwrap_or(false);
    if reject {
        return OffsetStrategy::Reject;
    }
    let window = choices.int_in_range(0..=2 * WINDOW.get()).unwrap_or(0);
    OffsetStrategy::Bounded {
        max_window: NonZeroU32::new(window).unwrap_or(WINDOW),
    }
}
