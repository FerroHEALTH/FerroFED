// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Arbitrary bytes as the `OPTIONS {base}/` body of a federation gateway
//! (§7a.2). A body that reads must also survive its own encoding unchanged.
//! An `Err` is the correct answer; a panic is the bug.

#![no_main]

use libfuzzer_sys::fuzz_target;
use openehr_federation::options::OptionsRoot;

fuzz_target!(|data: &[u8]| {
    let Ok(options) = serde_json::from_slice::<OptionsRoot>(data) else {
        return;
    };
    let Ok(encoded) = serde_json::to_vec(&options) else {
        panic!("an OPTIONS body that was read does not encode");
    };
    let Ok(decoded) = serde_json::from_slice::<OptionsRoot>(&encoded) else {
        panic!("an encoded OPTIONS body does not read back");
    };
    assert_eq!(
        options, decoded,
        "the OPTIONS body changes across its own encoding"
    );
});
