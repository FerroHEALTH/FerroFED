// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The PMIR identity feed of `[pmir]`: the ITI-94 subscription kept at a
//! harness Patient Identity Registry and shown on `/health/dependencies`, the
//! ITI-93 route that applies a merge to the resolution bindings only when it
//! is authenticated and well formed, the hygiene of the identifiers it
//! carries, and the configuration it refuses (track 8 of §16.3, Annex A.4;
//! PMIR 1.6.0 §2:3.93, §2:3.94).

mod config;
mod feed;
mod subscription;

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use axum::Router;
use ferrofed_identity::session::SessionKey;
use ferrofed_registry::id::{EhrId, NodeId};
use ferrofed_server::config::Config;
use ferrofed_server::state::AppState;

use crate::facade::{registry, settings_with_room};

/// The `ehr_id` domain of node A at the PIX Manager, which scopes a merge.
pub(crate) const DOMAIN_A: &str = "urn:oid:2.999.1.910";

/// The `ehr_id` domain of node B.
pub(crate) const DOMAIN_B: &str = "urn:oid:2.999.1.920";

/// A synthetic `ehr_id` at node A.
pub(crate) const EHR_A: &str = "9a9a9a9a-9a9a-4a9a-8a9a-9a9a9a9a9a9a";

/// Another synthetic `ehr_id` at node A.
pub(crate) const EHR_A2: &str = "9c9c9c9c-9c9c-4c9c-8c9c-9c9c9c9c9c9c";

/// The feed token the harness Registry and the gateway agreed out of band.
pub(crate) const TOKEN: &str = "Qz7feedtoken";

/// The path the feed is served at.
pub(crate) const PATH: &str = "/pmir/feed";

/// A gateway with the identity feed: its state and its router.
pub(crate) struct Gateway {
    pub(crate) state: Arc<AppState>,
    pub(crate) app: Router,
}

impl Gateway {
    /// Records the session `caller` resolving the patient to `ehr_ids` at
    /// node A, as a query would (§12.5.1 step 2).
    pub(crate) fn bind(&self, caller: &str, ehr_ids: &[&str]) -> Result<(), Box<dyn Error>> {
        let federation = self.state.federation().ok_or("a federation")?;
        let node = NodeId::new("node-a")?;
        let ehr_ids = ehr_ids
            .iter()
            .map(|ehr_id| EhrId::new(*ehr_id))
            .collect::<Result<Vec<_>, _>>()?;
        federation.bindings().record(
            &SessionKey::new(caller),
            Instant::now(),
            ehr_ids.iter().map(|ehr_id| (&node, ehr_id)),
        );
        Ok(())
    }

    /// How many `ehr_id` bindings the gateway holds over every session.
    pub(crate) fn bound(&self) -> Result<usize, Box<dyn Error>> {
        Ok(self
            .state
            .federation()
            .ok_or("a federation")?
            .bindings()
            .len())
    }
}

/// The configuration text of a development gateway over node A and node B,
/// resolving through a PIX Manager that is never asked, with the `[pmir]`
/// table at the Registry `registry_url` sending to `callback_url`, plus
/// `extra` keys in that table.
pub(crate) fn text(
    dir: &Path,
    registry_url: &str,
    callback_url: &str,
    extra: &str,
) -> Result<String, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(
        &document,
        registry("http://127.0.0.1:9/a", "http://127.0.0.1:9/b", ""),
    )?;
    let document = toml::Value::String(document.display().to_string());
    Ok(format!(
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n[federation]\nper_node_timeout_ms = 2000\noverall_timeout_ms = 3000\nnode_selection = \"ask-all\"\nid = \"example-federation\"\n\n[[pixm.manager]]\nurl = \"http://127.0.0.1:9/fhir/\"\n\n[pixm.manager.members]\n\"node-a\" = \"{DOMAIN_A}\"\n\"node-b\" = \"{DOMAIN_B}\"\n\n[pmir]\nurl = \"{registry_url}\"\ncallback_url = \"{callback_url}\"\npath = \"{PATH}\"\nfeed_token = \"{TOKEN}\"\n{extra}"
    ))
}

/// The gateway `text` configures.
pub(crate) fn gateway(text: &str) -> Result<Gateway, Box<dyn Error>> {
    let settings =
        Config::from_sources(Some(&crate::support::signed(text)), &BTreeMap::new())?.resolve()?;
    let state = Arc::new(AppState::build(&settings)?);
    let app = ferrofed_server::router(Arc::clone(&state), &settings_with_room());
    Ok(Gateway { state, app })
}
