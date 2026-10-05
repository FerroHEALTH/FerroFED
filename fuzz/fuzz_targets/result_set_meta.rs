// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Arbitrary bytes as a federated `RESULT_SET`, the shape a node or another
//! gateway answers with: the ITS-REST decode, then `meta.federation` read out
//! of its metadata (§9.1). A `meta.federation` that reads must also survive
//! its own encoding unchanged, so the wire types never accept a member they
//! cannot write back. An `Err` is the correct answer; a panic is the bug.

#![no_main]

use libfuzzer_sys::fuzz_target;
use openehr_federation::envelope;
use openehr_federation::meta::FederationMeta;
use openehr_its::rest::generated::query::ResultSet;

fuzz_target!(|data: &[u8]| {
    let Ok(result_set) = serde_json::from_slice::<ResultSet>(data) else {
        return;
    };
    let Some(metadata) = result_set.meta.as_ref() else {
        return;
    };
    let Ok(meta) = envelope::read(metadata) else {
        return;
    };
    let Ok(encoded) = serde_json::to_vec(&meta) else {
        panic!("a meta.federation that was read does not encode");
    };
    let Ok(decoded) = serde_json::from_slice::<FederationMeta>(&encoded) else {
        panic!("an encoded meta.federation does not read back");
    };
    assert_eq!(
        meta, decoded,
        "meta.federation changes across its own encoding"
    );
});
