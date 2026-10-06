// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `report` job: the archive names the build, the release and the
//! redacted configuration, reads every live part of a running gateway, and
//! never carries a patient identifier, a clinical payload, a credential or
//! a URL credential (§5.4.1, N33). No specification governs the report:
//! our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::io::Read as _;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_registry::incident::{Detection, Incident};
use ferrofed_server::config::Config;
use ferrofed_server::report::{ROOT, Report};
use ferrofed_server::state::AppState;
use http::header;
use tokio::net::TcpListener;

use crate::run::binary;
use crate::support::{OPERATOR_SCOPE, auth_toml, bearer, operator_bearer, signed};

type TestResult = Result<(), Box<dyn Error>>;

/// The synthetic patient the development cross-reference resolves.
const PATIENT: &str = "SENTINEL-PATIENT-r705";

/// The namespace of [`PATIENT`], an example OID.
const NAMESPACE: &str = "urn:oid:2.999.1.1";

/// The `ehr_id` [`PATIENT`] resolves to at `node-a`.
const EHR_ID: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";

/// A value inside a committed composition.
const CLINICAL: &str = "SENTINEL-CLINICAL-r705";

/// Every canary the configuration holds: the name and password of a
/// professional's basic credential, the userinfo, path, query and fragment
/// of a URL, a namespace under a field no list keeps, and the federation id,
/// a string no list keeps.
const CREDENTIALS: [&str; 9] = [
    "Dr SENTINEL-PROFESSIONAL-r705",
    "SENTINEL-PASSWORD-r705",
    "SENTINEL-USER-r705",
    "SENTINEL-PASS-r705",
    "SENTINEL-QUERY-r705",
    "SENTINEL-PATH-r705",
    "SENTINEL-FRAGMENT-r705",
    "2.999.4242",
    "SENTINEL-FEDERATION-r705",
];

/// The `ehr_id` of an integrity incident the operator surface reports.
const INCIDENT_EHR_ID: &str = "c0ffee00-0000-4000-8000-000000000705";

/// The `creating_system_id` of an integrity incident the operator surface
/// reports.
const INCIDENT_SYSTEM: &str = "sentinel-system-r705.example.org";

/// A registry of one node whose endpoint nothing listens on.
const REGISTRY: &str = r#"
[[organisation]]
id = "org-a"

[[node]]
id = "node-a"
organisation = "org-a"
system_id = "cdr-a.example.org"

[[endpoint]]
id = "node-a-pub"
node = "node-a"
url = "http://127.0.0.1:9/a"
connection_type = "openehr-rest-query"
managing_organisation = "org-a"
"#;

/// Returns the configuration of a gateway on `gateway` with its admin
/// listener on `admin`, reading the registry `document`.
fn configuration(gateway: &str, admin: &str, document: &Path) -> Result<String, Box<dyn Error>> {
    let document = toml::Value::String(document.display().to_string());
    let [
        professional,
        professional_password,
        user,
        password,
        query,
        path,
        fragment,
        namespace,
        federation,
    ] = CREDENTIALS;
    let text = format!(
        "profile = \"development\"\n\n\
         [server]\nlisten = \"{gateway}\"\n\n\
         [telemetry]\notlp_endpoint = \"http://{user}:{password}@127.0.0.1:4317/{path}?key={query}#{fragment}\"\n\n\
         [metrics]\nlisten = \"{admin}\"\nscrape_token = \"SENTINEL-SCRAPE-r705\"\n\n\
         [registry]\ndocument = {document}\n\n\
         [federation]\nid = \"{federation}\"\nnode_selection = \"ask-all\"\ndefault_namespace = \"urn:oid:{namespace}\"\n\n\
         [credentials.\"node-a-pub\"]\nuser = \"{professional}\"\npassword = \"{professional_password}\"\n\n\
         [[dev.crossref]]\nnamespace = \"{NAMESPACE}\"\nvalue = \"{PATIENT}\"\nmember = \"node-a\"\nehr_id = \"{EHR_ID}\"\n{}operator_scope = \"{OPERATOR_SCOPE}\"\n",
        auth_toml()?
    );
    Ok(signed(&text))
}

/// Reads every file of the archive at `path`, by its path below the root.
fn files(path: &Path) -> Result<BTreeMap<String, Vec<u8>>, Box<dyn Error>> {
    let mut archive = tar::Archive::new(std::fs::File::open(path)?);
    let mut files = BTreeMap::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let name = entry.path()?.display().to_string();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        let name = name
            .strip_prefix(&format!("{ROOT}/"))
            .ok_or("every file sits under the root")?
            .to_owned();
        files.insert(name, bytes);
    }
    Ok(files)
}

/// Asserts no file in `files`, and no byte of the archive at `path`,
/// carries a patient identifier, a clinical value or a credential.
fn nothing_leaks(path: &Path, files: &BTreeMap<String, Vec<u8>>) -> TestResult {
    let archive = String::from_utf8_lossy(&std::fs::read(path)?).into_owned();
    for value in [
        PATIENT,
        NAMESPACE,
        EHR_ID,
        CLINICAL,
        INCIDENT_EHR_ID,
        INCIDENT_SYSTEM,
        "SENTINEL",
    ]
    .into_iter()
    .chain(CREDENTIALS)
    {
        assert!(!archive.contains(value), "the archive carries {value}");
        for (name, bytes) in files {
            assert!(
                !String::from_utf8_lossy(bytes).contains(value),
                "{name} carries {value}"
            );
        }
    }
    Ok(())
}

/// Sends the gateway at `gateway` the patient's query and a composition, so
/// both have passed through the process the report reads.
async fn exercise(gateway: std::net::SocketAddr) -> TestResult {
    let client = reqwest::Client::new();
    let query = format!(
        "SELECT c FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    );
    let asked = client
        .post(format!("http://{gateway}/v1/query/aql"))
        .header(header::AUTHORIZATION, bearer()?)
        .json(&BTreeMap::from([("q", query)]))
        .send()
        .await?;
    assert!(!asked.status().is_success(), "the node is unreachable");
    let committed = client
        .post(format!("http://{gateway}/v1/ehr/{EHR_ID}/composition"))
        .header(header::AUTHORIZATION, bearer()?)
        .header(header::CONTENT_TYPE, "application/json")
        .body(format!(
            "{{\"_type\":\"COMPOSITION\",\"name\":{{\"_type\":\"DV_TEXT\",\"value\":\"{CLINICAL}\"}}}}"
        ))
        .send()
        .await?;
    assert!(!committed.status().is_success(), "the node is unreachable");
    Ok(())
}

/// Emits two incidents, one naming an `ehr_id` and one a
/// `creating_system_id`, which the operator surface answers and the archive
/// leaves out.
fn emit_incidents() -> TestResult {
    Incident::EhrIdCollision {
        ehr_id: INCIDENT_EHR_ID.parse()?,
        detection: Detection::AskAll,
        claimants: vec!["node-a-pub".parse()?],
    }
    .emit();
    Incident::LearnedCreatingSystemConflict {
        creating_system_id: INCIDENT_SYSTEM.parse()?,
        first: "node-a-pub".parse()?,
        second: "node-a-pub".parse()?,
    }
    .emit();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_running_gateway_is_reported_without_a_patient_identifier_or_a_credential() -> TestResult
{
    let dir = tempfile::tempdir()?;
    let document = dir.path().join("registry.toml");
    std::fs::write(&document, REGISTRY)?;
    let gateway_listener = TcpListener::bind("127.0.0.1:0").await?;
    let admin_listener = TcpListener::bind("127.0.0.1:0").await?;
    let gateway = gateway_listener.local_addr()?;
    let admin = admin_listener.local_addr()?;
    let file = dir.path().join("ferrofed.toml");
    std::fs::write(
        &file,
        configuration(&gateway.to_string(), &admin.to_string(), &document)?,
    )?;
    let (config, tree) = Config::load_with_tree(Some(&file))?;
    let settings = config.resolve()?;
    let state = Arc::new(AppState::build(&settings)?);
    state.lifecycle().booted();
    let app = ferrofed_server::router(Arc::clone(&state), &settings.server);
    tokio::spawn(ferrofed_server::serve_until(
        gateway_listener,
        app,
        Duration::ZERO,
        std::future::pending(),
    ));
    let (_, admin_app) =
        ferrofed_server::admin::listener(&settings, &state).ok_or("an admin listener")?;
    tokio::spawn(ferrofed_server::admin::serve(admin_listener, admin_app));

    exercise(gateway).await?;
    emit_incidents()?;

    let operator = operator_bearer()?;
    let token = secrecy::SecretString::from(
        operator
            .strip_prefix("Bearer ")
            .ok_or("a bearer value")?
            .to_owned(),
    );
    let report = Report::collect(&settings, &tree, Some(&token)).await?;
    assert_eq!(Vec::<String>::new(), {
        report
            .missing()
            .iter()
            .map(|missing| format!("{}: {}", missing.path, missing.reason))
            .collect::<Vec<_>>()
    });
    let out = dir.path().join("report.tar");
    report.write(&out)?;
    let files = files(&out)?;
    assert_eq!(
        vec![
            "build.json",
            "configuration.toml",
            "health/dependencies.json",
            "health/readiness.json",
            "incidents.json",
            "manifest.json",
            "metrics.txt",
            "release.json",
        ],
        files.keys().collect::<Vec<_>>()
    );
    nothing_leaks(&out, &files)?;
    let configuration = String::from_utf8(files["configuration.toml"].clone())?;
    assert!(
        configuration.contains("[[dev.crossref]]"),
        "{configuration}"
    );
    assert!(
        configuration.contains("node_selection = \"ask-all\""),
        "{configuration}"
    );
    let incidents = String::from_utf8(files["incidents.json"].clone())?;
    assert!(incidents.contains("EhrIdCollision"), "{incidents}");
    assert!(incidents.contains("node-a-pub"), "{incidents}");
    let manifest = String::from_utf8(files["manifest.json"].clone())?;
    let digest = ferrofed_server::conformance::fixture::sha256(&files["metrics.txt"]);
    assert!(manifest.contains(&digest), "the manifest hashes every file");
    assert!(
        String::from_utf8(files["metrics.txt"].clone())?.contains("ferrofed_"),
        "the metrics were read"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stopped_gateway_is_reported_with_every_live_part_named_missing() -> TestResult {
    let dir = tempfile::tempdir()?;
    let out = dir.path().join("report.tar");
    let out_arg = out.display().to_string();
    // A data-keyed table and a URL with a path, in the PIXm
    // binding, beside a bearer token, a namespace and the cross-reference.
    let pixm = if cfg!(feature = "binding-ihe") {
        "\n[[pixm.manager]]\nurl = \"http://127.0.0.1:9/fhir/SENTINEL-PIX-PATH\"\n\n\
         [pixm.manager.members]\n\"SENTINEL-MEMBER\" = \"urn:oid:2.999.7777\"\n"
    } else {
        ""
    };
    let toml = format!(
        "profile = \"development\"\n\n[server]\nlisten = \"127.0.0.1:9\"\n\n\
         [federation]\ndefault_namespace = \"urn:oid:2.999.4242\"\n\n\
         [credentials.\"node-a-pub\"]\nbearer_token = \"SENTINEL-TOKEN-r705\"\n\n\
         [[dev.crossref]]\nnamespace = \"{NAMESPACE}\"\nvalue = \"{PATIENT}\"\nmember = \"node-a\"\nehr_id = \"{EHR_ID}\"\n{pixm}"
    );
    let output = tokio::task::spawn_blocking(move || {
        binary(&["report", "--out", &out_arg], &toml).map_err(|error| error.to_string())
    })
    .await??;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(Some(0), output.status.code(), "{stderr}");
    for missing in [
        "health/readiness.json",
        "health/dependencies.json",
        "incidents.json",
        "metrics.txt",
    ] {
        assert!(stderr.contains(missing), "{missing} is named: {stderr}");
    }
    let files = files(&out)?;
    assert!(!files.contains_key("metrics.txt"), "no empty stand-in");
    let manifest = String::from_utf8(files["manifest.json"].clone())?;
    assert!(manifest.contains("\"missing\""), "{manifest}");
    assert!(manifest.contains("metrics.listen is not set"), "{manifest}");
    nothing_leaks(&out, &files)?;
    let archive = String::from_utf8_lossy(&std::fs::read(&out)?).into_owned();
    assert!(
        !archive.contains("2.999.7777"),
        "a member's namespace leaked"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn an_existing_archive_is_never_overwritten() -> TestResult {
    let dir = tempfile::tempdir()?;
    let out = dir.path().join("report.tar");
    std::fs::write(&out, "kept")?;
    let out_arg = out.display().to_string();
    let output = tokio::task::spawn_blocking(move || {
        binary(
            &["report", "--out", &out_arg],
            "[server]\nlisten = \"127.0.0.1:9\"\n",
        )
        .map_err(|error| error.to_string())
    })
    .await??;
    assert_eq!(Some(1), output.status.code());
    assert_eq!("kept", std::fs::read_to_string(&out)?);
    Ok(())
}
