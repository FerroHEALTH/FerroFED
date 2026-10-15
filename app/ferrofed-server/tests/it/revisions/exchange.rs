// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A revision exported as TOML and TOML imported as a dry-run revision: the
//! export is an attachment that holds the console's members and sections and
//! every secret as its reference alone, an import of an export is the same
//! configuration with no change against it, nothing is applied by an import,
//! and an import that sets a secret inline, a key of the bootstrap core or a
//! resource the file provides is refused. No specification governs this:
//! our own design.

use std::error::Error;

use axum::body::Body;
use ferrofed_registry::resource::{Resource, SecretChange, SectionRequest};
use ferrofed_registry::revision::{Imported, Origin, Revision, RevisionId};
use ferrofed_registry::secret::WriteOnly;
use http::{Method, Request, StatusCode, header};

use super::{Gateway, TestResult, operator};
use crate::support::{error_body, send_as_is};

/// A synthetic secret value no export may hold.
const VALUE: &str = "synthetic-export-secret-5be1";

/// An answer: its status, its headers and its body.
type Answer = (StatusCode, http::HeaderMap, String);

impl Gateway {
    /// Sends `request` as the operator and returns the whole answer.
    async fn exchange(
        &self,
        request: http::request::Builder,
        body: Body,
    ) -> Result<Answer, Box<dyn Error>> {
        let request = request
            .header(header::AUTHORIZATION, operator()?)
            .body(body)?;
        let response = send_as_is(self.admin.clone(), request).await?;
        let (parts, body) = response.into_parts();
        let bytes = axum::body::to_bytes(body, 4 * 1024 * 1024).await?;
        Ok((
            parts.status,
            parts.headers,
            String::from_utf8(bytes.to_vec())?,
        ))
    }

    /// Exports revision `id`.
    async fn export(&self, id: u64) -> Result<Answer, Box<dyn Error>> {
        self.exchange(
            Request::builder().uri(format!("/admin/config/revisions/{id}/export")),
            Body::empty(),
        )
        .await
    }

    /// Imports `text` as TOML on `base`.
    async fn import(&self, base: u64, text: &str) -> Result<Answer, Box<dyn Error>> {
        self.exchange(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/admin/config/import?base={base}&reason=from%20version%20control"
                ))
                .header(header::CONTENT_TYPE, "application/toml"),
            Body::from(text.to_owned()),
        )
        .await
    }

    /// The revision numbered `number` the store holds.
    fn held(&self, number: u64) -> Result<Revision, Box<dyn Error>> {
        let id = RevisionId::new(number).ok_or("a revision number")?;
        Ok(self
            .revisions
            .store()
            .revision(id)?
            .ok_or("the revision is held")?)
    }

    /// The newest revision's number.
    fn head(&self) -> Result<u64, Box<dyn Error>> {
        Ok(self
            .revisions
            .store()
            .head()?
            .ok_or("a revision is held")?
            .get())
    }
}

/// Imports the export of revision `id` on the newest revision, and returns
/// the import's answer.
async fn round_trip(gateway: &Gateway, id: u64) -> Result<Imported, Box<dyn Error>> {
    let (status, headers, exported) = gateway.export(id).await?;
    assert_eq!(StatusCode::OK, status, "{exported}");
    assert_eq!(
        Some("no-store"),
        headers
            .get(header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok()),
        "an export is never cached"
    );
    assert_eq!(
        Some(format!("attachment; filename=\"ferrofed-revision-{id}.toml\"").as_str()),
        headers
            .get(header::CONTENT_DISPOSITION)
            .and_then(|v| v.to_str().ok()),
        "an export is an attachment"
    );
    assert!(
        exported.starts_with(&format!("# FerroFED configuration revision {id}, saved at")),
        "{exported}"
    );
    assert!(exported.contains("bootstrap core"), "{exported}");
    let (status, _, text) = gateway.import(gateway.head()?, &exported).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    Ok(serde_json::from_str(&text)?)
}

#[tokio::test]
async fn an_exported_revision_imports_as_itself_and_is_not_applied() -> TestResult {
    let gateway = Gateway::start("")?;
    let imported = round_trip(&gateway, 1).await?;
    let original = gateway.held(1)?;
    assert_eq!(Origin::Import, imported.revision.origin);
    assert_eq!(original.toml, imported.revision.toml);
    assert_eq!(original.console, imported.revision.console);
    assert_eq!(1, imported.diff.from.get());
    assert_eq!(imported.revision.id, imported.diff.to);
    assert!(imported.diff.changes.is_empty(), "{:?}", imported.diff);
    assert_eq!(
        Some(RevisionId::FIRST),
        gateway
            .revisions
            .store()
            .current()?
            .map(|mark| mark.revision)
    );
    let (status, text) = gateway
        .post(
            &format!("/admin/config/revisions/{}/apply", imported.revision.id),
            None,
        )
        .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    Ok(())
}

#[tokio::test]
async fn a_console_member_exports_and_imports_with_its_revision() -> TestResult {
    let gateway = Gateway::start("")?;
    let member = r#"{"base": 1, "reason": "add C", "member": {
        "organisation": {"id": "org-c"},
        "node": {"id": "node-c", "system_id": "cdr-c.example.org"},
        "endpoints": [{"id": "node-c-pub", "url": "http://127.0.0.1:9/c",
                       "connection_type": "openehr-rest-query"}]}}"#;
    let (status, text) = gateway
        .send(
            (Method::PUT, "/admin/config/members/node-c"),
            Some(member.to_owned()),
            Some(&operator()?),
        )
        .await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let imported = round_trip(&gateway, 2).await?;
    assert_eq!(gateway.held(2)?.toml, imported.revision.toml);
    assert!(
        imported.revision.toml.contains("node-c"),
        "{}",
        imported.revision.toml
    );
    let changed: Vec<&str> = imported
        .diff
        .changes
        .iter()
        .map(|change| change.key.as_str())
        .collect();
    assert!(
        changed
            .iter()
            .all(|key| key.starts_with("registry.members")),
        "{changed:?}"
    );
    Ok(())
}

#[tokio::test]
async fn the_export_holds_a_secret_as_its_reference_alone() -> TestResult {
    let secrets = tempfile::tempdir()?;
    let dir = toml::Value::String(secrets.path().display().to_string());
    let gateway = Gateway::configured((&format!("secrets_dir = {dir}\n"), ""), None)?;
    let body = serde_json::to_string(&SectionRequest {
        base: Some(RevisionId::FIRST),
        reason: String::from("the node's token"),
        settings: String::new(),
        secrets: vec![SecretChange {
            key: String::from("bearer_token"),
            value: Some(WriteOnly::new(VALUE)),
            file: None,
            clear: false,
        }],
    })?;
    let (status, text) = gateway
        .send(
            (Method::PUT, "/admin/config/credentials/node-a-pub"),
            Some(body),
            Some(&operator()?),
        )
        .await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let (status, headers, exported) = gateway.export(2).await?;
    assert_eq!(StatusCode::OK, status, "{exported}");
    assert!(
        headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|media| media.starts_with("application/toml")),
        "{headers:?}"
    );
    assert!(!exported.contains(VALUE), "the export holds the secret");
    let table: toml::Table = toml::from_str(&exported)?;
    let reference = table["credentials"]["node-a-pub"]["bearer_token_file"]
        .as_str()
        .ok_or("the secret is exported as its reference")?;
    assert!(
        reference.starts_with(&secrets.path().display().to_string()),
        "{reference}"
    );
    let imported = round_trip(&gateway, 2).await?;
    assert_eq!(gateway.held(2)?.toml, imported.revision.toml);
    assert_eq!(
        vec![Resource::Credentials(String::from("node-a-pub"))],
        imported.revision.console
    );
    Ok(())
}

/// Asserts that importing `text` on revision 1 is refused with `status` and
/// `code`, saves nothing, and answers no part of `secret` when it names one.
async fn refused(
    gateway: &Gateway,
    text: &str,
    (status, code): (StatusCode, &str),
    secret: &str,
) -> TestResult {
    let (answered, _, body) = gateway.import(1, text).await?;
    assert_eq!(status, answered, "{body}");
    assert_eq!(code, error_body(&body)?.code, "{body}");
    assert!(secret.is_empty() || !body.contains(secret), "{body}");
    assert_eq!(1, gateway.head()?, "a refused import saves nothing");
    Ok(())
}

#[tokio::test]
async fn an_import_with_an_inline_secret_is_refused() -> TestResult {
    let gateway = Gateway::start("")?;
    let text = format!(
        "{}\n[credentials.\"node-a-pub\"]\nbearer_token = \"{VALUE}\"\n",
        gateway.held(1)?.toml
    );
    refused(
        &gateway,
        &text,
        (StatusCode::UNPROCESSABLE_ENTITY, "configuration-invalid"),
        VALUE,
    )
    .await
}

#[tokio::test]
async fn an_import_with_a_key_of_the_bootstrap_core_is_refused() -> TestResult {
    let gateway = Gateway::start("")?;
    let text = format!("profile = \"production\"\n{}", gateway.held(1)?.toml);
    refused(
        &gateway,
        &text,
        (StatusCode::UNPROCESSABLE_ENTITY, "configuration-invalid"),
        "",
    )
    .await
}

#[tokio::test]
async fn an_import_that_changes_a_credential_the_file_provides_is_refused() -> TestResult {
    let secrets = tempfile::tempdir()?;
    let password = secrets.path().join("node-a");
    std::fs::write(&password, "synthetic-file-password")?;
    let gateway = Gateway::start(&format!(
        "\n[credentials.\"node-a-pub\"]\nuser = \"synthetic-user\"\npassword_file = {}\n",
        toml::Value::String(password.display().to_string())
    ))?;
    let mut table: toml::Table = toml::from_str(&gateway.held(1)?.toml)?;
    let credentials = table
        .get_mut("credentials")
        .and_then(|value| value.get_mut("node-a-pub"))
        .and_then(toml::Value::as_table_mut)
        .ok_or("the file's credentials")?;
    credentials.insert(String::from("user"), toml::Value::from("another-user"));
    let changed = toml::to_string(&table)?;
    refused(
        &gateway,
        &changed,
        (StatusCode::CONFLICT, "resource-file-owned"),
        "",
    )
    .await?;
    table.remove("credentials");
    refused(
        &gateway,
        &toml::to_string(&table)?,
        (StatusCode::CONFLICT, "resource-file-owned"),
        "",
    )
    .await
}

#[tokio::test]
async fn an_import_with_a_member_the_file_provides_is_refused() -> TestResult {
    let gateway = Gateway::start("")?;
    let text = format!(
        "{}\n[[registry.members]]\norganisation = {{ id = \"org-x\" }}\n\
         node = {{ id = \"node-a\", system_id = \"cdr-x.example.org\" }}\n\
         endpoints = [{{ id = \"node-x-pub\", url = \"http://127.0.0.1:9/x\", connection_type = \"openehr-rest-query\" }}]\n",
        gateway.held(1)?.toml
    );
    refused(
        &gateway,
        &text,
        (StatusCode::CONFLICT, "resource-file-owned"),
        "",
    )
    .await
}

#[tokio::test]
async fn an_import_names_its_media_type_its_reason_and_the_newest_base() -> TestResult {
    let gateway = Gateway::start("")?;
    let text = gateway.held(1)?.toml;
    let (status, _, body) = gateway
        .exchange(
            Request::builder()
                .method(Method::POST)
                .uri("/admin/config/import?base=1&reason=r")
                .header(header::CONTENT_TYPE, "application/json"),
            Body::from(text.clone()),
        )
        .await?;
    assert_eq!(StatusCode::UNSUPPORTED_MEDIA_TYPE, status, "{body}");
    assert_eq!("media-type-unsupported", error_body(&body)?.code);
    let (status, _, body) = gateway
        .exchange(
            Request::builder()
                .method(Method::POST)
                .uri("/admin/config/import?base=1")
                .header(header::CONTENT_TYPE, "application/toml"),
            Body::from(text.clone()),
        )
        .await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{body}");
    assert_eq!("body-invalid", error_body(&body)?.code);
    let large = "#".repeat(1_048_577);
    let (status, _, body) = gateway.import(1, &large).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{body}");
    assert_eq!("body-invalid", error_body(&body)?.code);
    let (status, _, body) = gateway.import(1, &text).await?;
    assert_eq!(StatusCode::CREATED, status, "{body}");
    let (status, _, body) = gateway.import(1, &text).await?;
    assert_eq!(StatusCode::CONFLICT, status, "{body}");
    assert_eq!("revision-stale", error_body(&body)?.code);
    let (status, _, body) = gateway.export(99).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{body}");
    assert_eq!("revision-unknown", error_body(&body)?.code);
    Ok(())
}
