// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 11, integrity: a versioned write no step of §12.5.1 routes is
//! refused `400 target-required`, with no endpoint acting, and never probed
//! for (§12.4, §12.5.1, §16.3 track 11; N41; CP-33).
//!
//! The collision half needs one `ehr_id` seeded at two members, a fixture
//! §16.3 makes the harness operator's to create because §12b.2 forbids a
//! node to reach it, so a run against a deployment never creates it; the
//! end-to-end suite does. That nobody is probed is judged on node-side
//! capture.

use http::{Method, StatusCode, header};
use openehr_federation::headers::ENDPOINT;

use crate::conformance::client::{Gateway, Reply, ask, request};
use crate::conformance::{Failure, ensure, ensure_eq};

/// Returns the versioned object `version`, a version uid, is a version of.
///
/// # Errors
///
/// Returns [`Failure::Check`] for a uid with no object id.
pub fn object_of(version: &str) -> Result<&str, Failure> {
    version
        .split("::")
        .next()
        .filter(|object| !object.is_empty())
        .ok_or_else(|| Failure::Check(format!("{version} names no versioned object")))
}

/// Holds that a versioned write nothing routes is refused.
///
/// An update naming `version` in `If-Match` and a delete of `version`, both
/// under `ehr_id`, which nothing routes, are each refused
/// `400 target-required` with no endpoint acting (§12.5.1, N41); returns
/// the refusals.
///
/// # Errors
///
/// Returns [`Failure`] naming the first expectation that did not hold.
pub async fn versioned_writes_unrouted<G: Gateway>(
    gateway: &G,
    ehr_id: &str,
    version: &str,
    composition: &str,
) -> Result<Vec<Reply>, Failure> {
    let if_match = format!("\"{version}\"");
    let writes = [
        request(
            Method::PUT,
            &format!("/v1/ehr/{ehr_id}/composition/{}", object_of(version)?),
            &[(header::IF_MATCH.as_str(), if_match.as_str())],
            Some(("application/json", composition.as_bytes().to_vec())),
        )?,
        request(
            Method::DELETE,
            &format!("/v1/ehr/{ehr_id}/composition/{version}"),
            &[],
            None,
        )?,
    ];
    let mut replies = Vec::with_capacity(writes.len());
    for write in writes {
        let what = format!("{} {}", write.method(), write.uri().path());
        let refused = ask(gateway, write).await?;
        refused.expect(
            StatusCode::BAD_REQUEST,
            &format!("N41: {what} is never ask-all-probed"),
        )?;
        ensure_eq(
            &"target-required".to_owned(),
            &refused.code()?,
            "N41: the refusal's code",
        )?;
        ensure(refused.field(ENDPOINT).is_none(), || {
            format!("{what}: no endpoint acted")
        })?;
        replies.push(refused);
    }
    Ok(replies)
}
