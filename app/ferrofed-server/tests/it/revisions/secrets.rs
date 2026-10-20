// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A revision never holds a secret value: a secret written inline is
//! refused by its key and never echoed, and one held by its `_file`
//! reference is stored and returned as the reference alone, its content
//! reaching neither the store nor any answer. No specification governs
//! this: our own design.

use http::StatusCode;

use super::{Gateway, TestResult, rest};
use crate::support::error_body;

/// A synthetic secret value no answer and no store may hold.
const INLINE: &str = "synthetic-inline-secret-5b1d";

/// The synthetic content of a secret file.
const IN_FILE: &str = "synthetic-file-secret-8e47";

/// Whether the store file of `gateway` holds `needle` anywhere.
fn stored(gateway: &Gateway, needle: &str) -> Result<bool, std::io::Error> {
    let bytes = std::fs::read(&gateway.store)?;
    Ok(bytes
        .windows(needle.len())
        .any(|window| window == needle.as_bytes()))
}

#[tokio::test]
async fn a_secret_written_inline_is_refused_and_never_echoed() -> TestResult {
    let gateway = Gateway::start("")?;
    for table in [
        format!("\n[credentials.\"node-a-pub\"]\nuser = \"gateway\"\npassword = \"{INLINE}\"\n"),
        format!("\n[credentials.\"node-a-pub\"]\nuser = \"u\"\npassword = \"{INLINE}\"\n"),
        format!("\n[telemetry]\notlp_endpoint = \"http://u:{INLINE}@127.0.0.1:4317\"\n"),
    ] {
        let toml = rest(&gateway.one(), &table);
        let (status, text) = gateway.save(Some(1), "an inline secret", &toml).await?;
        assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
        assert_eq!("configuration-invalid", error_body(&text)?.code);
        assert!(!text.contains(INLINE), "{text}");
        assert!(text.contains("_file sibling"), "{text}");
    }
    let (_, history) = gateway.get("/admin/config/revisions").await?;
    assert!(!history.contains(INLINE), "{history}");
    assert!(!stored(&gateway, INLINE)?, "the store never holds it");
    Ok(())
}

#[tokio::test]
async fn a_secret_held_by_reference_is_stored_as_the_reference_alone() -> TestResult {
    let gateway = Gateway::start("")?;
    let secret = gateway.dir.path().join("node-a-token");
    std::fs::write(&secret, IN_FILE)?;
    let reference = toml::Value::String(secret.display().to_string());
    let toml = rest(
        &gateway.one(),
        &format!(
            "\n[credentials.\"node-a-pub\"]\nuser = \"gateway\"\npassword_file = {reference}\n"
        ),
    );
    let (status, text) = gateway.save(Some(1), "a token by reference", &toml).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    assert!(text.contains("password_file"), "{text}");
    assert!(!text.contains(IN_FILE), "{text}");
    let (status, text) = gateway
        .post("/admin/config/revisions/2/apply", None)
        .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert!(!text.contains(IN_FILE), "{text}");
    for path in [
        "/admin/config",
        "/admin/config/revisions",
        "/admin/config/revisions/2",
        "/admin/config/revisions/1/diff/2",
    ] {
        let (status, text) = gateway.get(path).await?;
        assert_eq!(StatusCode::OK, status, "{path}: {text}");
        assert!(!text.contains(IN_FILE), "{path}: {text}");
    }
    assert!(
        !stored(&gateway, IN_FILE)?,
        "the store holds the reference alone"
    );
    Ok(())
}
