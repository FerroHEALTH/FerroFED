// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The test that holds every key of a maximal configuration to being
//! applied by a reload or named as needing a restart.

use ferrohealth_config::units;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::path::Path;

use ferrofed_testkit::listener::ListenerCertificates;
use ferrofed_testkit::oauth::Curve;
use toml::{Table, Value};

use crate::config::Config;
use crate::config::settings::Settings;
use crate::reload::restart::{effective, needs_restart};

type TestResult = Result<(), Box<dyn Error>>;

/// The maximal configuration, with `@DIR@` standing for the directory
/// its files are in.
const MAXIMAL: &str = r#"
profile = "development"

[server]
listen = "127.0.0.1:8080"
base_path = "/fed"
trusted_proxies = ["10.0.0.1"]
forwarded_header = "x-forwarded-for"
request_timeout = "30s"
header_read_timeout = "20s"
idle_timeout = "95s"
drain_delay = "10ms"
shutdown_timeout = "40s"
body_limit = "1MiB"
max_concurrent_requests = 512
retry_after = "2s"
compression = true
compression_min_size = "1KiB"

[gateway]
public_url = "https://gateway.example.org/fed"
replica = "replica-a"
bindings_drain_timeout = '5s'

[server.caller_rate]
requests_per_second = 10
burst = 20

[server.tls]
certificate_file = "@DIR@/listener.crt"
key_file = "@DIR@/listener.key"
client_auth = "required"
client_ca_file = "@DIR@/client-ca.pem"
reload_interval = "1h"
min_version = "1.2"
healthcheck_identity_file = "@DIR@/client.pem"

[telemetry]
format = "json"
filter = "info"
environment = "production"

[telemetry.otlp]
endpoint = "https://otel.example.org:4317"
traces = true
metrics = true
sample_ratio = 0.5

[telemetry.otlp.tls]
client_identity_file = "@DIR@/collector-client.pem"
trust_roots_file = "@DIR@/collector-ca.pem"

[registry]
document = "@DIR@/registry.toml"
format = "toml"

[federation]
id = "example-federation"
per_node_timeout = '10s'
overall_timeout = '20s'
default_namespace = "urn:oid:2.999.1"
binding_ttl = '15min'
binding_capacity = 1000
ehr_index_capacity = 1000
node_selection = "localized"
best_effort = true
offset_strategy = "bounded"
max_offset_window = 100
max_node_rows = 500
decomposable_aggregates = ["COUNT", "SUM"]
demographic_endpoint = "node-a-query"
fan_out_template_upload = true
fan_out_stored_queries = true
max_in_flight_per_node = 16
max_node_answer_size = '1MiB'

[federation.localization]
on_failure = "closed"
timeout = '1s'

[federation.consent]
disclose = true
emergency = "apply"

[credentials."node-a-query".credentials.oauth]
token_endpoint = "https://as.example.org/token"
client_id = "ferrofed"
client_auth = "private_key_jwt"
private_key_file = "@DIR@/signing.pem"

[credentials."node-b-query"]
credentials.user = "ferrofed"
credentials.password_file = "@DIR@/password"

[credentials."node-b-query".tls]
client_identity_file = "@DIR@/client.pem"
trust_roots_file = "@DIR@/client-ca.pem"

[[pixm.manager]]
endpoint.url = "https://pix.example.org/fhir/"
method = "get"

[pixm.manager.members]
"node-a" = "urn:oid:2.999.10"

[pixm.manager.endpoint.credentials]
user = "ferrofed"
password_file = "@DIR@/token"

[pixm.namespaces]
"urn:oid:2.999.1" = "urn:oid:2.999.1"

[pdqm]
endpoint.url = "https://pdq.example.org/fhir/"
transaction = "iti-78"
master = "urn:oid:2.999.1"
endpoint.timeout = '2s'

[pdqm.namespaces]
"urn:oid:2.999.7" = "urn:oid:2.999.7"

[xcpd]
sender_device = "2.999.40.1"
home_community = "2.999.40"
audit = "log"

[[xcpd.gateway]]
endpoint.url = "https://xcpd.example.org/rg"
device = "2.999.50.1"

[xcpd.communities]
"2.999.50" = "node-b"

[pmir]
endpoint.url = "https://pmir.example.org/fhir/"
callback_url = "https://gateway.example.org/fed/pmir/feed"
path = "/pmir/feed"
feed_clients = ["synthetic-scraper"]
endpoint.timeout = '2s'
check_interval = '1min'
on_drain = "keep"

[nl_gf.nvi]
endpoint.url = "https://nvi.example.org/fhir/"
namespaces = ["urn:oid:2.999.1"]

[nl_gf.nvi.custodians]
"urn:oid:2.999.60" = "node-a"

[nl_gf.mitz]
endpoint.url = "https://mitz.example.org/fhir/"
namespaces = ["urn:oid:2.999.1"]
purpose = "TREAT"
data_categories = ["GGC002"]
endpoint.timeout = '1s'

[nl_gf.mitz.holders]
"node-a" = { type = "V6", ura = "90000001" }

[stored_queries]
backend = "redb"
path = "@DIR@/stored.redb"

[store]
url_file = "@DIR@/store-url"
ca_file = "@DIR@/collector-ca.pem"
max_connections = 4
timeout = '10s'
sweep_interval = '1min'

[metrics]
listen = "127.0.0.1:9090"
scrape_clients = ["synthetic-scraper"]

[metrics.tls]
certificate_file = "@DIR@/listener.crt"
key_file = "@DIR@/listener.key"

[signing]
key_file = "@DIR@/signing.pem"
jwks_uri = "https://gateway.example.org/fed/.well-known/jwks.json"
node_jwks_cache = '5min'
rotation_overlap = '10min'

[auth]
product_scope = "__ferrofed.operator"

[[auth.issuer]]
issuer = "https://idp.example.org"
kind = "persons"
profile = "rfc9068"
audience = ["https://gateway.example.org/fed"]
clock_skew = '1min'
jwks_max_age = '5min'
jwks_refresh_interval = '30s'
timeout = '2s'
first_party_clients = ["ferrofed-console"]

[trust.purpose_of_use]
required = true

[[trust.issuer]]
issuer = "https://idp.example.org"
backend_clients = ["backend-a"]

[[access_log.emergency_purpose]]
system = "http://terminology.hl7.org/CodeSystem/v3-ActReason"
code = "ETREAT"

[audit]
destination = "log"

[usage_report]
enabled = false
endpoint = "https://report.example.org/v1/report"
state_dir = "@DIR@/usage-report"
deployment = "compose"

[config]
ui = "managed"
backend = "redb"
path = "@DIR@/revisions.redb"
poll = '2s'
confirm = '2min'
approval = ["consent"]
secrets_dir = "@DIR@/secrets"
"#;

/// Writes every file [`MAXIMAL`] names into `dir`, each secret ending in
/// `tag`.
fn files(dir: &Path, tag: &str, listener: &ListenerCertificates) -> TestResult {
    listener.write(dir)?;
    // NOTE: the collector's TLS is compared by content, so each directory holds its own set.
    let collector = ListenerCertificates::generate()?;
    std::fs::write(
        dir.join("collector-client.pem"),
        collector.client_identity(),
    )?;
    std::fs::write(dir.join("collector-ca.pem"), collector.ca())?;
    std::fs::write(
        dir.join("signing.pem"),
        Curve::P384.pem().map_err(|_unavailable| "a key")?,
    )?;
    std::fs::write(dir.join("registry.toml"), "")?;
    for secret in ["token", "password"] {
        std::fs::write(dir.join(secret), format!("synthetic-{secret}-{tag}"))?;
    }
    std::fs::write(
        dir.join("store-url"),
        format!("postgres://ferrofed@db.example.org:5432/ferrofed-{tag}?sslmode=require\n"),
    )?;
    std::fs::create_dir_all(dir.join("secrets"))?;
    Ok(())
}

/// The configuration `table` states, when it loads.
fn load(table: &Table) -> Option<Config> {
    let text = toml::to_string(table).ok()?;
    Config::from_sources(Some(&text), &BTreeMap::new()).ok()
}

/// The settings `table` resolves to, when it resolves.
fn resolve(table: &Table) -> Option<Settings> {
    load(table)?.resolve().ok()
}

/// One step of a key path: a table key or an array index.
#[derive(Debug, Clone)]
enum Step {
    Key(String),
    Index(usize),
}

/// The path of every leaf of `value` under `path`: a scalar, or an array
/// of scalars as one leaf; an array of tables is walked by index.
fn leaves(path: &[Step], value: &Value, out: &mut Vec<Vec<Step>>) {
    let step = |step: Step| [path, &[step]].concat();
    match value {
        Value::Table(table) => {
            for (key, value) in table {
                leaves(&step(Step::Key(key.clone())), value, out);
            }
        }
        Value::Array(items) if items.iter().all(Value::is_table) && !items.is_empty() => {
            for (index, value) in items.iter().enumerate() {
                leaves(&step(Step::Index(index)), value, out);
            }
        }
        _ => out.push(path.to_vec()),
    }
}

/// The dotted key `path` names, an array index in brackets.
fn key(path: &[Step]) -> String {
    let mut key = String::new();
    for step in path {
        match step {
            Step::Key(name) if key.is_empty() => key.push_str(name),
            Step::Key(name) => {
                key.push('.');
                key.push_str(name);
            }
            Step::Index(index) => {
                key.push('[');
                key.push_str(&index.to_string());
                key.push(']');
            }
        }
    }
    key
}

/// The value at `path` in `value`, mutable.
fn at<'tree>(value: &'tree mut Value, path: &[Step]) -> Option<&'tree mut Value> {
    path.iter().try_fold(value, |value, step| match step {
        Step::Key(name) => value.as_table_mut()?.get_mut(name),
        Step::Index(index) => value.as_array_mut()?.get_mut(*index),
    })
}

/// `tree` with the leaf at `path` set to `value`, or removed when
/// `value` is `None`.
fn changed(tree: &Table, path: &[Step], value: Option<Value>) -> Option<Table> {
    let mut root = Value::Table(tree.clone());
    let (last, parent) = path.split_last()?;
    let parent = at(&mut root, parent)?;
    match (last, value) {
        (Step::Key(name), Some(value)) => {
            parent.as_table_mut()?.insert(name.clone(), value);
        }
        (Step::Key(name), None) => {
            parent.as_table_mut()?.remove(name)?;
        }
        (Step::Index(index), Some(value)) => *parent.as_array_mut()?.get_mut(*index)? = value,
        (Step::Index(_), None) => return None,
    }
    match root {
        Value::Table(table) => Some(table),
        _ => None,
    }
}

/// Values of an enumeration, or of a key whose value another key holds
/// to, that a generic change cannot find.
const ALTERNATIVES: &[(&str, &str)] = &[
    ("server.listen", "127.0.0.1:8081"),
    ("server.forwarded_header", "forwarded"),
    ("telemetry.format", "pretty"),
    ("federation.node_selection", "ask-all"),
    ("federation.offset_strategy", "reject"),
    ("federation.localization.on_failure", "ask-all"),
    ("federation.consent.emergency", "pass-to-node"),
    ("pixm.manager[0].method", "post"),
    ("pdqm.transaction", "iti-119"),
    ("nl_gf.mitz.purpose", "COC"),
    ("xcpd.audit", "off"),
    ("pmir.on_drain", "unsubscribe"),
    ("metrics.listen", "127.0.0.1:9091"),
    ("audit.destination", "off"),
    ("registry.format", "fhir"),
    ("stored_queries.backend", "files"),
    ("config.ui", "off"),
    ("usage_report.deployment", "helm"),
    ("server.tls.client_auth", "optional"),
    ("server.tls.min_version", "1.3"),
];

/// The values a change of the leaf `key`, now `value`, tries in order: a
/// file in `from` is the same file in `to`.
fn alternatives(key: &str, value: &Value, from: &str, to: &str) -> Vec<Value> {
    let named = ALTERNATIVES
        .iter()
        .filter(|(name, _)| *name == key)
        .map(|(_, value)| Value::String((*value).to_owned()));
    let generic = match value {
        Value::Boolean(flag) => vec![Value::Boolean(!flag)],
        Value::Integer(number) => [number + 1, number * 2, number - 1]
            .into_iter()
            .map(Value::Integer)
            .collect(),
        Value::Float(number) => vec![Value::Float(number / 2.0)],
        Value::String(text) if text.parse::<units::Duration>().is_ok() => {
            let millis = text
                .parse::<units::Duration>()
                .map_or(0, units::Duration::as_millis);
            [
                millis * 2,
                millis + 1000,
                millis.saturating_sub(1000).max(1),
            ]
            .into_iter()
            .map(|millis| Value::String(units::Duration::from_millis(millis).to_string()))
            .collect()
        }
        Value::String(text) if text.parse::<units::ByteSize>().is_ok() => {
            let bytes = text
                .parse::<units::ByteSize>()
                .map_or(0, units::ByteSize::bytes);
            [bytes * 2, bytes + 1024, bytes.saturating_sub(1024).max(1)]
                .into_iter()
                .map(|bytes| Value::String(units::ByteSize::from_bytes(bytes).to_string()))
                .collect()
        }
        Value::String(text) if text.starts_with(from) => {
            vec![Value::String(text.replacen(from, to, 1))]
        }
        Value::String(text) => [
            text.replace("example.org", "example.net"),
            format!("{text}1"),
            format!("{text}-x"),
        ]
        .into_iter()
        .map(Value::String)
        .collect(),
        Value::Array(items) => {
            let mut shorter = items.clone();
            shorter.pop();
            let mut tries = vec![Value::Array(shorter)];
            if let Some(first) = items.first() {
                tries.extend(alternatives(key, first, from, to).into_iter().map(|first| {
                    let mut items = items.clone();
                    items.splice(..1, [first]);
                    Value::Array(items)
                }));
            }
            tries
        }
        Value::Datetime(_) | Value::Table(_) => Vec::new(),
    };
    named.chain(generic).collect()
}

/// Whether `key` lies in one of the `sections` a reload applies.
fn in_section(key: &str, sections: &[&str]) -> bool {
    sections.iter().any(|section| {
        key.strip_prefix(section)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(['.', '[']))
    })
}

/// The keys a reload refuses a change of, so it is never applied in
/// part: the profile ([`ReloadError::Profile`](crate::reload::ReloadError::Profile)).
const REFUSED: &[&str] = &["profile"];

/// The keys that only bound another key at load and set nothing the
/// gateway runs on.
const CHECKED_ONLY: &[&str] = &["signing.node_jwks_cache"];

/// The keys another key of the maximal configuration holds to the one
/// value it has, so no change of the key alone loads: an issuer that
/// `[[trust.issuer]]` and `[[auth.issuer]]` both name, an issuer's `kind`
/// and `profile`, each one of a closed set of names, and a node grant's
/// `client_auth`, which takes no client secret and presents a certificate
/// only with one configured.
const HELD: &[&str] = &[
    "auth.issuer[0].issuer",
    "auth.issuer[0].kind",
    "auth.issuer[0].profile",
    "config.backend",
    "config.ui",
    "credentials.node-a-query.credentials.oauth.client_auth",
    "stored_queries.backend",
    "trust.issuer[0].issuer",
];

/// The configuration the walk starts from, the files it names in `from`.
struct Start<'walk> {
    tree: Table,
    config: Config,
    boot: Settings,
    resolved: String,
    unchanged: String,
    from: &'walk str,
    to: &'walk str,
}

impl<'walk> Start<'walk> {
    fn new(tree: Table, from: &'walk str, to: &'walk str) -> Result<Self, Box<dyn Error>> {
        let config = load(&tree).ok_or("the configuration loads")?;
        let boot = resolve(&tree).ok_or("the configuration resolves")?;
        let again = resolve(&tree).ok_or("the configuration resolves")?;
        let resolved = format!("{boot:?}");
        if resolved != format!("{again:?}") {
            return Err("resolving the same configuration twice differs".into());
        }
        let unchanged = format!("{:?}", effective(&boot, again));
        Ok(Self {
            tree,
            config,
            boot,
            resolved,
            unchanged,
            from,
            to,
        })
    }

    /// The settings of `tree` with the leaf at `path` set to `value`,
    /// or removed, when that changes the configuration and resolves.
    fn fresh(&self, path: &[Step], value: Option<Value>) -> Option<Settings> {
        let tree = changed(&self.tree, path, value)?;
        (load(&tree)? != self.config)
            .then(|| resolve(&tree))
            .flatten()
    }

    /// The changes of the leaf at `path`: the first other value the
    /// settings show, else the first that resolves, and its removal.
    fn changes(&self, path: &[Step], key: &str) -> Result<Vec<Settings>, String> {
        let mut root = Value::Table(self.tree.clone());
        let value = at(&mut root, path).ok_or("a leaf")?.clone();
        let mut invisible = None;
        let mut changes = Vec::new();
        for candidate in alternatives(key, &value, self.from, self.to) {
            let Some(fresh) = self.fresh(path, Some(candidate)) else {
                continue;
            };
            if format!("{fresh:?}") == self.resolved {
                invisible.get_or_insert(fresh);
            } else {
                changes.push(fresh);
                break;
            }
        }
        if changes.is_empty() {
            changes.push(invisible.ok_or_else(|| key.to_owned())?);
        }
        changes.extend(
            self.fresh(path, None)
                .filter(|fresh| format!("{fresh:?}") != self.resolved),
        );
        Ok(changes)
    }

    /// Every key whose change a reload neither applies nor names as
    /// needing a restart, with what the reload did, and every key no
    /// other value of resolves in this form.
    fn silent(&self) -> (Vec<String>, BTreeSet<String>) {
        let reloadable = crate::binding::reloadable();
        let mut paths = Vec::new();
        leaves(&[], &Value::Table(self.tree.clone()), &mut paths);
        let mut silent = Vec::new();
        let mut held = BTreeSet::new();
        for path in paths {
            let key = key(&path);
            if REFUSED.contains(&key.as_str()) {
                continue;
            }
            let changes = match self.changes(&path, &key) {
                Ok(changes) => changes,
                Err(key) => {
                    held.insert(key);
                    continue;
                }
            };
            for fresh in changes {
                let visible = format!("{fresh:?}") != self.resolved;
                let named = needs_restart(&self.boot, &fresh);
                let applied = format!("{:?}", effective(&self.boot, fresh)) != self.unchanged;
                let held = match (applied, named.is_empty()) {
                    (true, true) | (false, false) => true,
                    (true, false) => false,
                    (false, true) => {
                        !visible
                            && (in_section(&key, &reloadable)
                                || CHECKED_ONLY.contains(&key.as_str()))
                    }
                };
                if !held {
                    silent.push(format!("{key}: applied {applied}, restart {named:?}"));
                }
            }
        }
        (silent, held)
    }
}

/// The path of the dotted `key`, which names no array element.
fn path(key: &str) -> Vec<Step> {
    key.split('.')
        .map(|step| Step::Key(step.to_owned()))
        .collect()
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "test assertions in a test that returns its setup errors"
)]
fn every_key_is_applied_or_named_as_needing_a_restart() -> TestResult {
    let root = tempfile::tempdir()?;
    let listener = ListenerCertificates::generate()?;
    let (from, to) = (root.path().join("a"), root.path().join("b"));
    for (dir, tag) in [(&from, "a"), (&to, "b")] {
        std::fs::create_dir_all(dir)?;
        files(dir, tag, &listener)?;
    }
    let (from, to) = (from.display().to_string(), to.display().to_string());
    let tree: Table = MAXIMAL.replace("@DIR@", &from).parse()?;
    // NOTE: no specification governs this: our own design; with `gateway.public_url` the
    // two keys under it derive from it, so each form is walked on its own.
    let derived = ["signing.jwks_uri", "pmir.callback_url"]
        .into_iter()
        .try_fold(tree.clone(), |tree, key| changed(&tree, &path(key), None))
        .ok_or("the derived keys are set")?;
    let written = changed(&tree, &path("gateway.public_url"), None).ok_or("a public URL")?;
    let mut silent = Vec::new();
    let mut unchangeable: Option<BTreeSet<String>> = None;
    for form in [derived, written] {
        let (found, held) = Start::new(form, &from, &to)?.silent();
        silent.extend(found);
        unchangeable = Some(match unchangeable {
            Some(before) => before.intersection(&held).cloned().collect(),
            None => held,
        });
    }
    assert!(silent.is_empty(), "{silent:#?}");
    assert_eq!(
        Some(HELD.iter().map(|key| (*key).to_owned()).collect()),
        unchangeable,
        "the keys no other value of resolves in any form"
    );
    Ok(())
}

// TEMPORARY timing probe for #1834; removed before the fix lands.
#[test]
#[expect(clippy::print_stderr, clippy::expect_used, reason = "a temporary probe")]
fn timing_probe_1834() -> TestResult {
    let root = tempfile::tempdir()?;
    let listener = ListenerCertificates::generate()?;
    let dir = root.path().join("a");
    std::fs::create_dir_all(&dir)?;
    files(&dir, "a", &listener)?;
    let tree: Table = MAXIMAL.replace("@DIR@", &dir.display().to_string()).parse()?;
    let started = std::time::Instant::now();
    for _ in 0..20 {
        resolve(&tree).expect("resolves");
    }
    let resolves = started.elapsed() / 20;
    let started = std::time::Instant::now();
    for _ in 0..20 {
        load(&tree).expect("loads");
    }
    let loads = started.elapsed() / 20;
    let provider = std::sync::Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let started = std::time::Instant::now();
    for _ in 0..20 {
        rustls_platform_verifier::Verifier::new_with_extra_roots([], std::sync::Arc::clone(&provider))
            .expect("a verifier");
    }
    let verifiers = started.elapsed() / 20;
    eprintln!("PROBE1834 resolve={resolves:?} load={loads:?} platform_verifier={verifiers:?}");
    Ok(())
}
