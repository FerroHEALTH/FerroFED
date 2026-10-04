// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The outbound gate a dispatch passes before its request is sent: the request
//! is refused when it would carry a withheld identifier (§5.4.1, N33).

use ferrofed_registry::id::EhrId;
use openehr_its::rest::client::Transport;

use crate::hygiene::{Composed, Outbound};

use super::{DispatchError, DispatchOptions, NodeClient, NodeQuery};

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
        options
            .conveyance()
            .holds_own(&self.endpoint, scope.as_ref())
            .map_err(|source| DispatchError::Conveyance {
                endpoint: self.endpoint.clone(),
                source,
            })?;
        if options.withheld.is_empty() {
            return Ok(());
        }
        let base = self.client.base();
        let mut url = base.clone();
        url.set_path(&format!("{}/query/aql", base.path()));
        let paging: Vec<String> = [query.offset, query.fetch]
            .into_iter()
            .flatten()
            .map(|number| number.to_string())
            .collect();
        // NOTE: §5.4.1, N33; exempting REQUEST_ID_HEADER is our own design: its
        // value is an OutboundId, minted with no client input, in which a short
        // all-hex identifier can occur by chance.
        let headers: [(&'static str, &str); 0] = [];
        let conveyed = options.conveyance().carried();
        let outbound = Outbound {
            aql: query.aql(),
            scope: query.scope.as_deref(),
            paging: &paging,
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
