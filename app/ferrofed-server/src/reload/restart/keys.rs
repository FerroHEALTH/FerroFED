// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The core's keys a reload leaves to the next start, each with how a
//! change of it is found, and the test that holds every key of a maximal
//! configuration to being applied by a reload or named as needing a
//! restart. No specification governs this: our own design.

use crate::config::settings::Settings;

/// Whether `fresh` names another stored-query store than the process
/// started with, or sets or unsets one.
fn stored_queries_changed(boot: &Settings, fresh: &Settings) -> bool {
    match (&boot.stored_queries, &fresh.stored_queries) {
        (Some(was), Some(now)) => !was.same_as(now),
        (None, None) => false,
        (Some(_), None) | (None, Some(_)) => true,
    }
}

/// Whether `fresh` names another configuration revision store than the
/// process started with, sets or unsets one, or reads it at another interval.
fn revisions_changed(boot: &Settings, fresh: &Settings) -> bool {
    boot.revision_poll != fresh.revision_poll
        || boot.revision_policy != fresh.revision_policy
        || match (&boot.revisions, &fresh.revisions) {
            (Some(was), Some(now)) => !was.same_as(now),
            (None, None) => false,
            (Some(_), None) | (None, Some(_)) => true,
        }
}

/// Whether `fresh` names other signing keys, another JWK Set location,
/// another assertion lifetime or another rotation overlap than the process
/// started with; the keys are compared by `kid`, never by their material.
fn signing_changed(boot: &Settings, fresh: &Settings) -> bool {
    let shape = |settings: &Settings| {
        settings.signing.as_ref().map(|signing| {
            (
                signing.keys.current().kid().to_owned(),
                signing.keys.retiring().map(|key| key.kid().to_owned()),
                signing.keys.next().map(|key| key.kid().to_owned()),
                signing.jwks_uri.as_str().to_owned(),
                signing.assertion_lifetime,
                signing.rotation_overlap,
            )
        })
    };
    shape(boot) != shape(fresh)
}

/// A key a reload does not apply, with whether its value differs between the
/// settings the process started with and the fresh ones.
pub(super) type RestartKey = (&'static str, fn(&Settings, &Settings) -> bool);

/// The core's keys a reload does not apply, in the order a refusal names
/// them.
pub(super) const RESTART_KEYS: &[RestartKey] = &[
    ("config", revisions_changed),
    ("signing", signing_changed),
    ("audit", |boot, fresh| boot.audit != fresh.audit),
    ("auth.audience", |boot, fresh| {
        boot.server.auth.audience != fresh.server.auth.audience
    }),
    ("server.listen", |boot, fresh| {
        boot.server.listen != fresh.server.listen
    }),
    ("server.tls", |boot, fresh| {
        boot.server.tls != fresh.server.tls
    }),
    ("server.public_url", |boot, fresh| {
        boot.server.public_url != fresh.server.public_url
    }),
    ("server.forwarding", |boot, fresh| {
        boot.server.forwarding != fresh.server.forwarding
    }),
    ("server.replica", |boot, fresh| {
        boot.server.replica != fresh.server.replica
    }),
    ("metrics.tls", |boot, fresh| {
        boot.metrics.tls != fresh.metrics.tls
    }),
    ("server.base_path", |boot, fresh| {
        boot.server.base_path != fresh.server.base_path
    }),
    ("server.request_timeout_ms", |boot, fresh| {
        boot.server.request_timeout != fresh.server.request_timeout
    }),
    ("server.drain_delay_ms", |boot, fresh| {
        boot.server.drain_delay != fresh.server.drain_delay
    }),
    ("server.shutdown_timeout_ms", |boot, fresh| {
        boot.server.shutdown_timeout != fresh.server.shutdown_timeout
    }),
    ("server.bindings_drain_timeout_ms", |boot, fresh| {
        boot.server.bindings_drain != fresh.server.bindings_drain
    }),
    ("server.body_limit_bytes", |boot, fresh| {
        boot.server.body_limit != fresh.server.body_limit
    }),
    ("server.overload", |boot, fresh| {
        boot.server.overload != fresh.server.overload
    }),
    ("telemetry.format", |boot, fresh| {
        boot.telemetry.format != fresh.telemetry.format
    }),
    ("telemetry.filter", |boot, fresh| {
        boot.telemetry.filter != fresh.telemetry.filter
    }),
    ("telemetry.otlp_endpoint", |boot, fresh| {
        boot.telemetry.otlp_endpoint != fresh.telemetry.otlp_endpoint
    }),
    ("telemetry.trace_sample_ratio", |boot, fresh| {
        boot.telemetry.trace_sample_ratio != fresh.telemetry.trace_sample_ratio
    }),
    ("federation.id", |boot, fresh| {
        boot.federation.id != fresh.federation.id
    }),
    ("federation.timeouts", |boot, fresh| {
        boot.federation.budget != fresh.federation.budget
    }),
    ("federation.default_namespace", |boot, fresh| {
        boot.federation.default_namespace != fresh.federation.default_namespace
    }),
    ("federation.binding_ttl_ms", |boot, fresh| {
        boot.federation.binding_ttl != fresh.federation.binding_ttl
    }),
    ("federation.binding_capacity", |boot, fresh| {
        boot.federation.binding_capacity != fresh.federation.binding_capacity
    }),
    ("federation.ehr_index_capacity", |boot, fresh| {
        boot.federation.ehr_index_capacity != fresh.federation.ehr_index_capacity
    }),
    ("federation.node_selection", |boot, fresh| {
        boot.federation.node_selection != fresh.federation.node_selection
    }),
    ("federation.best_effort", |boot, fresh| {
        boot.federation.best_effort != fresh.federation.best_effort
    }),
    ("federation.max_in_flight_per_node", |boot, fresh| {
        boot.federation.max_in_flight_per_node != fresh.federation.max_in_flight_per_node
    }),
    ("federation.max_node_answer_bytes", |boot, fresh| {
        boot.federation.max_node_answer_bytes != fresh.federation.max_node_answer_bytes
    }),
    ("federation.fan_out_template_upload", |boot, fresh| {
        boot.federation.fan_out_template_upload != fresh.federation.fan_out_template_upload
    }),
    ("federation.fan_out_stored_queries", |boot, fresh| {
        boot.federation.fan_out_stored_queries != fresh.federation.fan_out_stored_queries
    }),
    ("federation.offset", |boot, fresh| {
        boot.federation.offset != fresh.federation.offset
    }),
    ("federation.max_node_rows", |boot, fresh| {
        boot.federation.max_node_rows != fresh.federation.max_node_rows
    }),
    ("federation.decomposable_aggregates", |boot, fresh| {
        boot.federation.decomposable != fresh.federation.decomposable
    }),
    ("federation.demographic_endpoint", |boot, fresh| {
        boot.federation.demographic_endpoint != fresh.federation.demographic_endpoint
    }),
    ("stored_queries", stored_queries_changed),
    ("fhir", |boot, fresh| {
        boot.fhir.as_ref().map(|fhir| &fhir.written)
            != fresh.fhir.as_ref().map(|fhir| &fhir.written)
    }),
    ("eehrxf.terminology", |boot, fresh| {
        boot.fhir.as_ref().map(|fhir| &fhir.terminology_written)
            != fresh.fhir.as_ref().map(|fhir| &fhir.terminology_written)
    }),
    ("metrics.listen", |boot, fresh| {
        boot.metrics.listen != fresh.metrics.listen
    }),
    ("metrics.otlp_endpoint", |boot, fresh| {
        boot.metrics.otlp_endpoint != fresh.metrics.otlp_endpoint
    }),
    ("metrics.scrape_token", |boot, fresh| {
        boot.metrics.scrape_token != fresh.metrics.scrape_token
    }),
];

#[cfg(test)]
#[cfg(all(feature = "binding-ihe", feature = "binding-nl"))]
mod every_key {
    use std::collections::{BTreeMap, BTreeSet};
    use std::error::Error;
    use std::path::Path;

    use ferrofed_testkit::listener::ListenerCertificates;
    use ferrofed_testkit::oauth::es384_pem;
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
public_url = "https://gateway.example.org/fed"
trusted_proxies = ["10.0.0.1"]
forwarded_header = "x-forwarded-for"
request_timeout_ms = 30000
drain_delay_ms = 10
shutdown_timeout_ms = 40000
bindings_drain_timeout_ms = 5000
body_limit_bytes = 1048576
max_concurrent_requests = 512
overload_retry_after_s = 2
replica = "replica-a"

[server.caller_rate]
requests_per_second = 10
burst = 20

[server.tls]
certificate_file = "@DIR@/listener.crt"
key_file = "@DIR@/listener.key"
client_ca_file = "@DIR@/client-ca.pem"
healthcheck_identity_file = "@DIR@/client.pem"

[telemetry]
format = "json"
filter = "info"
otlp_endpoint = "http://otel.example.org:4318"
trace_sample_ratio = 0.5

[registry]
document = "@DIR@/registry.toml"
format = "toml"

[federation]
id = "example-federation"
per_node_timeout_ms = 10000
overall_timeout_ms = 20000
default_namespace = "urn:oid:2.999.1"
binding_ttl_ms = 900000
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
max_node_answer_bytes = 1048576

[federation.localization]
on_failure = "closed"
timeout_ms = 1000

[federation.consent]
disclose = true
emergency = "apply"

[credentials."node-a-query"]
bearer_token_file = "@DIR@/token"

[credentials."node-b-query"]
user = "ferrofed"
password_file = "@DIR@/password"
client_identity_file = "@DIR@/client.pem"
trust_roots_file = "@DIR@/client-ca.pem"

[[dev.crossref]]
namespace = "urn:oid:2.999.1"
value = "12345"
member = "node-a"
ehr_id = "7d44b88c-4199-4bad-97dc-d78268e01398"

[[pixm.manager]]
url = "https://pix.example.org/fhir/"
method = "get"

[pixm.manager.members]
"node-a" = "urn:oid:2.999.10"

[pixm.manager.credentials]
bearer_token_file = "@DIR@/token"

[pixm.namespaces]
"urn:oid:2.999.1" = "urn:oid:2.999.1"

[pdqm]
url = "https://pdq.example.org/fhir/"
transaction = "iti-78"
master = "urn:oid:2.999.1"
timeout_ms = 2000

[pdqm.namespaces]
"urn:oid:2.999.7" = "urn:oid:2.999.7"

[xcpd]
sender_device = "2.999.40.1"
home_community = "2.999.40"
audit = "log"

[[xcpd.gateway]]
url = "https://xcpd.example.org/rg"
device = "2.999.50.1"

[xcpd.communities]
"2.999.50" = "node-b"

[pmir]
url = "https://pmir.example.org/fhir/"
callback_url = "https://gateway.example.org/fed/pmir/feed"
path = "/pmir/feed"
feed_token_file = "@DIR@/feed-token"
timeout_ms = 2000
check_interval_s = 60
on_drain = "keep"

[nl_gf.nvi]
url = "https://nvi.example.org/fhir/"
namespaces = ["urn:oid:2.999.1"]

[nl_gf.nvi.custodians]
"urn:oid:2.999.60" = "node-a"

[nl_gf.mitz]
url = "https://mitz.example.org/fhir/"
namespaces = ["urn:oid:2.999.1"]
purpose = "TREAT"
data_categories = ["GGC002"]
timeout_ms = 1000

[nl_gf.mitz.holders]
"node-a" = { type = "V6", ura = "90000001" }

[stored_queries]
backend = "redb"
path = "@DIR@/stored.redb"

[metrics]
listen = "127.0.0.1:9090"
scrape_token_file = "@DIR@/scrape-token"
otlp_endpoint = "http://otel.example.org:4318"

[metrics.tls]
certificate_file = "@DIR@/listener.crt"
key_file = "@DIR@/listener.key"

[signing]
key_file = "@DIR@/signing.pem"
jwks_uri = "https://gateway.example.org/fed/.well-known/jwks.json"
assertion_lifetime_s = 60
node_jwks_cache_s = 300
rotation_overlap_s = 600

[auth]
audience = "https://gateway.example.org/fed"
clock_skew_s = 60
key_set_max_age_s = 300
key_set_refetch_s = 30
fetch_timeout_ms = 2000

[auth.purpose_of_use]
required = true

[[auth.issuer]]
issuer = "https://idp.example.org"
jwks_uri = "https://idp.example.org/jwks"
backend_clients = ["backend-a"]
operator_scope = "ferrofed:operator"

[[access_log.emergency_purpose]]
system = "http://terminology.hl7.org/CodeSystem/v3-ActReason"
code = "ETREAT"

[audit]
destination = "log"

[config]
ui = "managed"
backend = "redb"
path = "@DIR@/revisions.redb"
poll_ms = 2000
confirm_ms = 120000
approval = ["consent"]
secrets_dir = "@DIR@/secrets"
"#;

    /// Writes every file [`MAXIMAL`] names into `dir`, each secret ending in
    /// `tag`.
    fn files(dir: &Path, tag: &str, listener: &ListenerCertificates) -> TestResult {
        listener.write(dir)?;
        std::fs::write(
            dir.join("signing.pem"),
            es384_pem().map_err(|_unavailable| "a key")?,
        )?;
        std::fs::write(dir.join("registry.toml"), "")?;
        for secret in ["token", "password", "feed-token", "scrape-token"] {
            std::fs::write(dir.join(secret), format!("synthetic-{secret}-{tag}"))?;
        }
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
    const CHECKED_ONLY: &[&str] = &["signing.node_jwks_cache_s"];

    /// The keys another key of the maximal configuration holds to the one
    /// value it has, so no change of the key alone loads.
    const HELD: &[&str] = &["config.backend", "config.ui", "stored_queries.backend"];

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
        // NOTE: no specification governs this: our own design; with `server.public_url` the
        // two keys under it derive from it, so each form is walked on its own.
        let derived = ["signing.jwks_uri", "pmir.callback_url"]
            .into_iter()
            .try_fold(tree.clone(), |tree, key| changed(&tree, &path(key), None))
            .ok_or("the derived keys are set")?;
        let written = changed(&tree, &path("server.public_url"), None).ok_or("a public URL")?;
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
}
