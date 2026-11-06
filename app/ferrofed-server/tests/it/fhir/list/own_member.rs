// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The members the tests of a member's own documents run against: node A
//! serving the documents of one category itself, behind a capturing proxy,
//! beside its Query API, and node B, which serves none.

use std::collections::BTreeMap;
use std::sync::Arc;

use ferrofed_server::config::Config;
use ferrofed_server::state::AppState;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::pdqm_stub;
use ferrofed_testkit::proxy::{Bind, CapturingProxy};
use serde::Deserialize;
use serde_json::Value;
use serde_json::value::RawValue;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use super::super::{UID_B, node_holding, supplier, text_registered};
use super::{EHR_SYSTEM, entries};
use crate::facade::{EHR_A, EHR_B, settings_with_room};

/// The member's own logical id of its patient.
pub(super) const MEMBER_PATIENT: &str = "pat-a";

/// The member's `Patient` search, naming [`MEMBER_PATIENT`].
pub(super) const PATIENTS: &str = r#"{"resourceType":"Bundle","type":"searchset","entry":[{"fullUrl":"Patient/pat-a","resource":{"resourceType":"Patient","id":"pat-a"},"search":{"mode":"match"}}]}"#;

/// A `Patient` search that finds no patient.
pub(super) const NO_PATIENT: &str = r#"{"resourceType":"Bundle","type":"searchset"}"#;

/// The member's `Binary`, whose whitespace no serialiser would keep.
pub(super) const BINARY: &[u8] =
    b"{ \"resourceType\" :\"Binary\", \"contentType\": \"application/fhir+json\" }\n";

/// How node A's own endpoint is declared and what it holds.
#[derive(Debug, Clone, Copy)]
pub(super) struct Declared {
    /// The endpoint's id.
    pub(super) endpoint: &'static str,
    /// The category it serves.
    pub(super) category: &'static str,
    /// The document `Bundle` profile it serves the category in.
    pub(super) profile: &'static str,
    /// The logical id of its `Binary`.
    pub(super) binary: &'static str,
    /// The logical id of its `DocumentReference`.
    pub(super) document: &'static str,
    /// The `type` its `DocumentReference` search answers alone, or none to
    /// answer every `type`.
    pub(super) kind: Option<&'static str>,
}

/// The member's `DocumentReference` search, holding `document`, of the id
/// `declared` names.
pub(super) fn documents(declared: &Declared, document: &str) -> String {
    format!(
        r#"{{"resourceType":"Bundle","type":"searchset","entry":[{{"fullUrl":"DocumentReference/{}","resource":{document},"search":{{"mode":"match"}}}}]}}"#,
        declared.document
    )
}

/// A FHIR JSON answer of `body`.
pub(super) fn answered(body: &[u8]) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body.to_vec(), "application/fhir+json")
}

/// Node A serving the documents `declared` names itself, behind a capturing
/// proxy, beside its Query API, and node B, which serves none.
pub(super) struct Members {
    /// Node A's Query API.
    pub(super) a: Server,
    /// Node B's Query API.
    pub(super) b: Server,
    /// Node A's own endpoint.
    pub(super) own: Server,
    /// The proxy in front of it.
    pub(super) proxy: CapturingProxy,
    /// The demographics supplier.
    pdq: Server,
    /// How node A's own endpoint is declared.
    declared: Declared,
}

impl Members {
    /// The members, node A's own endpoint answering the `Patient` search
    /// with `patients`, the `DocumentReference` search with `documents` and
    /// the read of its `Binary` with [`BINARY`].
    pub(super) async fn start(
        declared: Declared,
        patients: ResponseTemplate,
        documents: ResponseTemplate,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let own = Server::start().await;
        Mock::given(method("GET"))
            .and(path("/fhir/Patient"))
            .and(query_param("identifier", format!("{EHR_SYSTEM}|{EHR_A}")))
            .respond_with(patients)
            .mount(&own)
            .await;
        let search = Mock::given(method("GET"))
            .and(path("/fhir/DocumentReference"))
            .and(query_param("patient", MEMBER_PATIENT));
        match declared.kind {
            Some(kind) => {
                search
                    .and(query_param("type", kind))
                    .respond_with(documents)
                    .mount(&own)
                    .await;
            }
            None => search.respond_with(documents).mount(&own).await,
        }
        Mock::given(method("GET"))
            .and(path(format!("/fhir/Binary/{}", declared.binary)))
            .respond_with(answered(BINARY))
            .mount(&own)
            .await;
        let proxy = CapturingProxy::start(own.uri(), Bind::Loopback).await?;
        Ok(Self {
            a: Server::start().await,
            b: node_holding(UID_B).await,
            own,
            proxy,
            pdq: supplier().await,
            declared,
        })
    }

    /// The members, node A holding the one document `document`.
    pub(super) async fn holding(
        declared: Declared,
        document: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::start(
            declared,
            answered(PATIENTS.as_bytes()),
            answered(documents(&declared, document).as_bytes()),
        )
        .await
    }

    /// The gateway over the members, with the `[federation]` keys
    /// `federation` and the tables `tables`.
    pub(super) fn gateway(
        &self,
        dir: &std::path::Path,
        (federation, tables): (&str, &str),
    ) -> Result<axum::Router, Box<dyn std::error::Error>> {
        let rows = [("node-a", EHR_A), ("node-b", EHR_B)];
        let declared = self.declared;
        let endpoint = format!(
            "\n[[endpoint]]\nid = \"{}\"\nnode = \"node-a\"\nurl = \"{}/fhir\"\n\
             connection_type = \"hl7-fhir-rest\"\nmanaging_organisation = \"org-a\"\n\
             categories = [\"{}\"]\nprofiles = [\"{}\"]\n\
             ehr_id_system = \"{EHR_SYSTEM}\"\n",
            declared.endpoint,
            self.proxy.origin(),
            declared.category,
            declared.profile
        );
        let text = text_registered(
            dir,
            (
                &self.a.uri(),
                &self.b.uri(),
                &pdqm_stub::base_url(&self.pdq),
            ),
            &rows,
            &endpoint,
            (federation, tables),
        )?;
        let settings =
            Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?
                .resolve()?;
        let state = Arc::new(AppState::build(&settings)?);
        Ok(ferrofed_server::router(state, &settings_with_room()))
    }

    /// The path and the query pairs of every request node A's own endpoint
    /// received, in order.
    pub(super) fn asked(&self) -> Vec<(String, Vec<(String, String)>)> {
        self.proxy
            .journal()
            .iter()
            .map(|request| {
                let query = request.query.as_deref().unwrap_or_default();
                let pairs = url::form_urlencoded::parse(query.as_bytes())
                    .map(|(name, value)| (name.into_owned(), value.into_owned()))
                    .collect();
                (request.path.clone(), pairs)
            })
            .collect()
    }
}

/// One entry of a `searchset`, its resource as the gateway wrote it.
#[derive(Deserialize)]
struct RawEntry {
    /// The resource's text.
    resource: Box<RawValue>,
}

/// A `searchset`, each resource as the gateway wrote it.
#[derive(Deserialize)]
struct RawSearchset {
    /// The entries.
    #[serde(default)]
    entry: Vec<RawEntry>,
}

/// The one document `text` lists, with the text the gateway wrote of the
/// member's document of the id `declared` names.
pub(super) fn listed(
    declared: &Declared,
    text: &str,
) -> Result<(Value, String), Box<dyn std::error::Error>> {
    let bundle: Value = serde_json::from_str(text)?;
    let matches = entries(&bundle, "match");
    let [reference] = matches.as_slice() else {
        return Err(format!("one document listed: {bundle}").into());
    };
    let raw: RawSearchset = serde_json::from_str(text)?;
    let id = format!("\"{}\"", declared.document);
    let written = raw
        .entry
        .iter()
        .map(|entry| entry.resource.get())
        .find(|resource| resource.contains(&id))
        .ok_or("the member's document")?
        .to_owned();
    Ok(((*reference).clone(), written))
}

/// The attachment URL of the listed `reference`.
pub(super) fn url_of(reference: &Value) -> Result<String, Box<dyn std::error::Error>> {
    Ok(reference
        .pointer("/content/0/attachment/url")
        .and_then(Value::as_str)
        .ok_or("an attachment url")?
        .to_owned())
}
