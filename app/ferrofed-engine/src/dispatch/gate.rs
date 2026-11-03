// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The outbound gate a dispatch passes before its request is sent: the request
//! is refused when it would carry a withheld identifier (§5.4.1, N33).

use ferrofed_registry::id::EhrId;
use openehr_its::rest::client::Transport;

use crate::hygiene::{Composed, Outbound};

use super::{DispatchError, DispatchOptions, NodeClient, NodeQuery};

/// What a request tells the node beside its path, as the outbound gate reads
/// it.
pub(super) struct Told<'a> {
    /// The AQL text the request carries, empty when it carries none.
    pub(super) aql: &'a str,
    /// The `ehr_id` the query is scoped to.
    pub(super) scope: Option<&'a str>,
    /// The paging parameters, as written.
    pub(super) paging: &'a [String],
}

impl<T: Transport> NodeClient<T> {
    /// The outbound gate: refuses `query` when the request it composes would
    /// carry a withheld identifier, or would tell the node a confined
    /// patient's `ehr_id` other than the one the query is scoped to (§5.4.1,
    /// N33).
    pub(super) fn gate(
        &self,
        query: &NodeQuery,
        options: &DispatchOptions,
    ) -> Result<(), DispatchError> {
        // NOTE: §7.1, the scope is the node's own ehr_id the rewrite wrote, so one that is no
        // HIER_OBJECT_ID composes the query for none, which a confined conveyance refuses.
        let scope = query
            .scope
            .as_deref()
            .and_then(|scope| EhrId::new(scope).ok());
        let paging: Vec<String> = [query.offset.map(u64::from), query.fetch]
            .into_iter()
            .flatten()
            .map(|number| number.to_string())
            .collect();
        let told = Told {
            aql: query.aql(),
            scope: query.scope.as_deref(),
            paging: &paging,
        };
        self.gate_told(scope.as_ref(), options, &told, |base| {
            format!("{base}/query/aql")
        })
    }

    /// The outbound gate over a request that tells the node `told` at the
    /// path `path` makes of the base path, composed for `ehr_id`: refused
    /// when a confined conveyance is not the patient's own at this endpoint,
    /// or when the request would carry a withheld identifier (§5.4.1, N33).
    pub(super) fn gate_told(
        &self,
        ehr_id: Option<&EhrId>,
        options: &DispatchOptions,
        told: &Told<'_>,
        path: impl FnOnce(&str) -> String,
    ) -> Result<(), DispatchError> {
        options
            .conveyance()
            .holds_own(&self.endpoint, ehr_id)
            .map_err(|source| DispatchError::Conveyance {
                endpoint: self.endpoint.clone(),
                source,
            })?;
        if options.withheld.is_empty() {
            return Ok(());
        }
        let base = self.client.base();
        let mut url = base.clone();
        url.set_path(&path(base.path()));
        // NOTE: §5.4.1, N33; exempting REQUEST_ID_HEADER is our own design: its
        // value is an OutboundId, minted with no client input, in which a short
        // all-hex identifier can occur by chance.
        let headers: [(&'static str, &str); 0] = [];
        let conveyed = options.conveyance().carried();
        let outbound = Outbound {
            aql: told.aql,
            scope: told.scope,
            paging: told.paging,
            url: &url,
            composed: Composed::default(),
            headers: &headers,
            conveyed: &conveyed,
        };
        match options.withheld.found_in(&outbound) {
            Some(part) => Err(DispatchError::Withheld {
                endpoint: self.endpoint.clone(),
                part,
            }),
            None => Ok(()),
        }
    }
}
