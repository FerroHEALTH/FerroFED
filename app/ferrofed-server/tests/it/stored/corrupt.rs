// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A stored-query store holding a row that does not read refuses the start,
//! and the log names the field and the kind of fault, never the held text.
//! The PostgreSQL store's case is in the end-to-end suite. No specification
//! governs this: our own design.

use std::net::TcpListener;
use std::path::Path;

use ferrofed_registry::definition::store::{
    DefinitionStore as _, Field, FieldFault, RowFault, StoreError,
};
use ferrofed_server::stored::embedded::RedbStore;
use redb::{Database, TableDefinition};

use super::TestResult;
use crate::facade::{EHR_A, EHR_B, crossref, registry};
use crate::run::binary;

/// A value a planted row carries, which no log line may hold.
const PLANTED: &str = "synthetic-planted-7731";

/// The embedded store's table of definitions, as the store lays it out.
const DEFINITIONS: TableDefinition<'static, (&str, &str), (&str, &str)> =
    TableDefinition::new("stored_query_definitions");

/// What `serve` writes when its stored queries are `stored_queries`, a
/// `[stored_queries]` table body, with the registry document in `dir`.
fn served(dir: &Path, stored_queries: &str) -> Result<String, Box<dyn std::error::Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(
        &document,
        registry("http://a.invalid", "http://b.invalid", ""),
    )?;
    let document = toml::Value::String(document.display().to_string());
    let taken = TcpListener::bind("127.0.0.1:0")?;
    let listen = taken.local_addr()?;
    let rows = crossref(&[("node-a", EHR_A), ("node-b", EHR_B)]);
    let config = format!(
        "profile = \"development\"\n\n[server]\nlisten = \"{listen}\"\n\n\
         [telemetry]\nformat = \"json\"\n\n[registry]\ndocument = {document}\n\n\
         [federation]\nid = \"example-federation\"\nnode_selection = \"ask-all\"\n\
         per_node_timeout_ms = 2000\noverall_timeout_ms = 3000\n\n\
         [stored_queries]\n{stored_queries}\n{rows}"
    );
    let output = binary(&["serve"], &config)?;
    drop(taken);
    assert_eq!(
        Some(i32::from(ferrofed_server::EXIT_CONFIG)),
        output.status.code(),
        "the start is refused"
    );
    Ok(format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

/// Asserts that no line of `logged` carries the planted value.
fn never_planted(logged: &str) {
    assert!(logged.contains("cannot start"), "the refusal is logged");
    for line in logged.lines() {
        assert!(
            !line.contains(PLANTED),
            "a log line carries the planted value"
        );
    }
}

#[test]
fn a_row_the_embedded_store_cannot_read_is_reported_by_field_and_kind() -> TestResult {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("definitions.redb");
    drop(RedbStore::open(&file)?);
    {
        let database = Database::create(&file)?;
        let write = database.begin_write()?;
        write
            .open_table(DEFINITIONS)?
            .insert(("org.example::planted", "1.0.0"), (PLANTED, PLANTED))?;
        write.commit()?;
    }
    let refused = RedbStore::open(&file)?
        .load()
        .err()
        .ok_or("the row does not read")?;
    assert!(
        matches!(
            refused,
            StoreError::Unreadable(RowFault {
                field: Field::Saved,
                kind: FieldFault::Form
            })
        ),
        "{refused:?}"
    );

    let path = toml::Value::String(file.display().to_string());
    let logged = served(dir.path(), &format!("backend = \"redb\"\npath = {path}"))?;
    never_planted(&logged);
    assert!(logged.contains("the saved instant"), "the field is named");
    Ok(())
}

#[test]
fn a_definition_file_the_files_store_refuses_is_logged_without_its_text() -> TestResult {
    let dir = tempfile::tempdir()?;
    let definitions = dir.path().join("definitions");
    let named = definitions.join("org.example::planted");
    std::fs::create_dir_all(&named)?;
    std::fs::write(named.join("1.0.0.aql"), format!("SELECT {PLANTED} FROM"))?;
    let path = toml::Value::String(definitions.display().to_string());
    let logged = served(dir.path(), &format!("backend = \"files\"\npath = {path}"))?;
    never_planted(&logged);
    assert!(logged.contains("1.0.0.aql"), "the file is named");
    Ok(())
}
