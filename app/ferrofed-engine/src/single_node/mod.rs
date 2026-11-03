// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The calls that send one request to one node, outside the fan-out of a
//! federated query.
//!
//! [`ehr`] creates and reads an EHR on one node for the admission check
//! (§12b.1), [`composition`] commits a composition the gateway composes into
//! one node's EHR, [`forward`] passes one client request to its owning node
//! once (§7a.3, N22, N31), [`probe`] sends the one read of §12.5.1 step 4
//! to each member, [`native`] asks a member's own exchange-format endpoint
//! for its patient summary, [`native_search`] searches that endpoint for a
//! patient's resources page by page, and [`native_create`] creates one
//! resource there. No specification governs the
//! grouping: our own design.

pub mod composition;
pub mod ehr;
pub mod forward;
pub mod native;
pub mod native_create;
pub mod native_search;
pub mod probe;

use url::Url;

use crate::dispatch::DispatchOptions;
use crate::hygiene::{Composed, Outbound, Part};

/// The URL of a request to `path` under `base`, as the outbound gate reads
/// it.
fn under(base: &Url, path: &str) -> Url {
    let mut url = base.clone();
    url.set_path(&format!("{}{path}", base.path().trim_end_matches('/')));
    url
}

/// The part of a request to `url`, composed as `composed` and carrying
/// `headers` and the caller claims `options` convey, that holds an
/// identifier `options` withhold (§5.4.1, N33).
fn withheld_part(
    options: &DispatchOptions,
    url: &Url,
    composed: Composed<'_>,
    headers: &[(&'static str, &str)],
) -> Option<Part> {
    let conveyed = options.conveyance().carried();
    options.withheld().found_in(&Outbound {
        aql: "",
        scope: None,
        paging: &[],
        url,
        composed,
        headers,
        conveyed: &conveyed,
    })
}
