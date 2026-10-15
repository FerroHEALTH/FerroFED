// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A plain save of a whole revision holds to the owner of every resource
//! as the console's own saves do: a change to, or the removal of, a member,
//! a credential or a binding section the file provides is refused `409`
//! (`resource-file-owned`) with nothing saved, and a save that sets each as
//! the file does is taken. No specification governs this: our own design.

use std::error::Error;

use http::StatusCode;

use super::{Gateway, TestResult};
use crate::support::error_body;

/// A gateway whose file provides the credentials of `node-a-pub` and the
/// consent binding section.
fn file_owned() -> Result<(Gateway, tempfile::TempDir), Box<dyn Error>> {
    let secrets = tempfile::tempdir()?;
    let password = secrets.path().join("node-a");
    std::fs::write(&password, "synthetic-file-password")?;
    let gateway = Gateway::start(&format!(
        "\n[credentials.\"node-a-pub\"]\nuser = \"synthetic-user\"\npassword_file = {}\n\
         \n[federation.consent]\ndisclose = true\n",
        toml::Value::String(password.display().to_string())
    ))?;
    Ok((gateway, secrets))
}

/// The file's revision 1, as a table.
fn first(gateway: &Gateway) -> Result<toml::Table, Box<dyn Error>> {
    let revision = gateway
        .revisions
        .store()
        .revision(ferrofed_registry::revision::RevisionId::FIRST)?
        .ok_or("the file's revision is held")?;
    Ok(toml::from_str(&revision.toml)?)
}

/// Asserts that a plain save of `table` on revision 1 is refused as a
/// change to a resource the file provides, and that nothing is saved.
async fn refused(gateway: &Gateway, table: &toml::Table) -> TestResult {
    let (status, text) = gateway
        .save(Some(1), "a whole revision", &toml::to_string(table)?)
        .await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    assert_eq!("resource-file-owned", error_body(&text)?.code, "{text}");
    assert_eq!(
        Some(1),
        gateway
            .revisions
            .store()
            .head()?
            .map(ferrofed_registry::revision::RevisionId::get),
        "a refused save saves nothing"
    );
    Ok(())
}

#[tokio::test]
async fn a_save_that_changes_a_file_owned_credential_is_refused() -> TestResult {
    let (gateway, _secrets) = file_owned()?;
    let mut table = first(&gateway)?;
    table
        .get_mut("credentials")
        .and_then(|value| value.get_mut("node-a-pub"))
        .and_then(toml::Value::as_table_mut)
        .ok_or("the file's credentials")?
        .insert(String::from("user"), toml::Value::from("another-user"));
    refused(&gateway, &table).await
}

#[tokio::test]
async fn a_save_that_removes_a_file_owned_binding_section_is_refused() -> TestResult {
    let (gateway, _secrets) = file_owned()?;
    let mut table = first(&gateway)?;
    table
        .get_mut("federation")
        .and_then(toml::Value::as_table_mut)
        .ok_or("the file's federation table")?
        .remove("consent");
    refused(&gateway, &table).await
}

#[tokio::test]
async fn a_save_that_sets_a_file_owned_member_is_refused() -> TestResult {
    let (gateway, _secrets) = file_owned()?;
    let mut table = first(&gateway)?;
    let member: toml::Table = toml::from_str(
        "organisation = { id = \"org-x\" }\n\
         node = { id = \"node-a\", system_id = \"cdr-x.example.org\" }\n\
         endpoints = [{ id = \"node-x-pub\", url = \"http://127.0.0.1:9/x\", connection_type = \"openehr-rest-query\" }]\n",
    )?;
    table
        .get_mut("registry")
        .and_then(toml::Value::as_table_mut)
        .ok_or("the file's registry table")?
        .insert(
            String::from("members"),
            toml::Value::Array(vec![toml::Value::Table(member)]),
        );
    refused(&gateway, &table).await
}

#[tokio::test]
async fn a_save_that_sets_each_file_owned_resource_as_the_file_does_is_taken() -> TestResult {
    let (gateway, _secrets) = file_owned()?;
    let mut table = first(&gateway)?;
    table
        .get_mut("federation")
        .and_then(toml::Value::as_table_mut)
        .ok_or("the file's federation table")?
        .insert(String::from("per_node_timeout_ms"), toml::Value::from(4000));
    let (status, text) = gateway
        .save(Some(1), "a longer wait", &toml::to_string(&table)?)
        .await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    Ok(())
}
