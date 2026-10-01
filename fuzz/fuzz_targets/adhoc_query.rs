// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Arbitrary bytes as the body of `POST {base}/v1/query/aql`, through the path
//! the façade runs: the ITS-REST `AdhocQueryExecute` decode, the
//! `query_parameters` intake, and the rewrite of §7.1. A decode error, an
//! intake error and a refusal are each the correct answer; a panic is the bug.

#![no_main]

use ferrofed_server::facade::intake;
use libfuzzer_sys::fuzz_target;
use openehr_federation::aql::{Context, Paging, Targeting, analyse};
use openehr_its::rest::generated::query::AdhocQueryExecute;

fuzz_target!(|data: &[u8]| {
    let Ok(request) = serde_json::from_slice::<AdhocQueryExecute>(data) else {
        return;
    };
    let Ok(parameters) = intake::parameters(request.query_parameters.as_ref()) else {
        return;
    };
    let paging = Paging {
        offset: request.offset,
        fetch: request.fetch,
    };
    let context = Context::new(Targeting::AskAll).with_default_namespace("urn:oid:2.999.1");
    let _analysis = analyse(&request.q, &parameters, paging, &context);
});
