#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The shared-crates guard (no specification governs this: our own design).
#
# Every FerroHEALTH product, its operator console included, takes its
# server plumbing, configuration, health family, secret handling, telemetry
# and usage report from the FerroSYS crates, and keeps no copy of its own
# beside them (#1397). This guard holds FerroFED to the crates it has moved
# onto:
#
#   - the root Cargo.toml pins each of them exactly (`= "=x.y.z"`), as a
#     product pins every family crate;
#   - no hand-written Rust source under app/, crates/ or tools/ (tests
#     excepted, which may mirror a document they read) defines again what
#     such a crate provides: a secret type over a SecretString or a
#     key-or-file reader (ferrohealth-secret); the indicator trait or the
#     lifecycle (ferrohealth-health); the report section, the reporter, the
#     state directory or the envelope types (ferrohealth-usage-report); a
#     TLS listener, its kept certificates, a serve loop, a drain, or a layer
#     of the stack (the request id, the panic catch, the timeout, the body
#     limit, the overload bound or the caller rate) of its own
#     (ferrohealth-server); the telemetry section, the console formats, the
#     request log, the sampler, a trace export, an OTLP or Prometheus
#     exporter or a panic hook of its own (ferrohealth-telemetry).
#
# A crate joins ADOPTED, with the definitions it retires in RETIRED, in the
# pull request that moves FerroFED onto it.
#
#   scripts/checks/shared-crates.sh [--self-test]
#
# Exit 0 = held. Exit 1 = a pin is missing or not exact, or a retired
# definition is back. Exit 2 = a usage error.
set -Eeuo pipefail
export LC_ALL=C

readonly SELF='scripts/checks/shared-crates.sh'

# The FerroSYS crates FerroFED runs on, by crates.io name.
readonly ADOPTED=(
  ferrohealth-config
  ferrohealth-secret
  ferrohealth-health
  ferrohealth-server
  ferrohealth-telemetry
  ferrohealth-usage-report
)

# One retired definition per line: the crate that provides it, a tab, and
# the extended regular expression that finds a local copy of it.
readonly RETIRED="ferrohealth-secret	(struct|enum)[[:space:]]+[A-Za-z_]+[[:space:]]*\\([[:space:]]*(pub[[:space:]]+)?(secrecy::)?SecretString[[:space:]]*\\)
ferrohealth-secret	fn[[:space:]]+(read_secret|require_secret|read_secret_file)[[:space:]]*[<(]
ferrohealth-health	trait[[:space:]]+HealthIndicator([^A-Za-z0-9_]|\$)
ferrohealth-health	struct[[:space:]]+Lifecycle([^A-Za-z0-9_]|\$)
ferrohealth-usage-report	struct[[:space:]]+(UsageReport|UsageReportSection|UsageReportSettings|StateDir|Reporter|SchemaV2|Window24h)([^A-Za-z0-9_]|\$)
ferrohealth-server	struct[[:space:]]+(TlsListener|Certificates|ReloadableTls)([^A-Za-z0-9_]|\$)
ferrohealth-server	fn[[:space:]]+(serve_until|drain_on|server_config)[[:space:]]*[<(]
ferrohealth-server	(CatchPanicLayer|SetRequestIdLayer|PropagateRequestIdLayer|RequestBodyLimitLayer|TimeoutLayer)([^A-Za-z0-9_]|\$)
ferrohealth-server	struct[[:space:]]+(Admission|CallerLimit)([^A-Za-z0-9_]|\$)
ferrohealth-telemetry	(struct|enum)[[:space:]]+(TelemetrySection|OtlpSection|SampleRatio|Traces|Format|Rendering|RequestLog)([^A-Za-z0-9_]|\$)
ferrohealth-telemetry	fn[[:space:]]+(sampler|collector_tls|method_label)[[:space:]]*[<(]
ferrohealth-telemetry	(opentelemetry_otlp|opentelemetry_prometheus)::
ferrohealth-telemetry	(std::panic::set_hook|fn[[:space:]]+install_hook[[:space:]]*[<(])
ferrohealth-telemetry	tracing_subscriber::fmt\(\)
ferrohealth-server	fn[[:space:]]+(same_origin|content_security_policy|shutdown_signal)[[:space:]]*[<(]
ferrohealth-config	fn[[:space:]]+(apply_override|env_value)[[:space:]]*[<(]"

# check ROOT: prints every finding under ROOT and returns 1 when there is one.
check() {
  local root="$1" found=0 crate pattern hits
  for crate in "${ADOPTED[@]}"; do
    if ! grep -Eq "^${crate}[[:space:]]*=[[:space:]]*(\"=[0-9]+\.[0-9]+\.[0-9]+\"|\{[^}]*version[[:space:]]*=[[:space:]]*\"=[0-9]+\.[0-9]+\.[0-9]+\")" "$root/Cargo.toml"; then
      echo "shared-crates: Cargo.toml does not pin ${crate} exactly (\"=x.y.z\")" >&2
      found=1
    fi
  done
  while IFS=$'\t' read -r crate pattern; do
    [ -n "$crate" ] || continue
    hits="$(cd "$root" && grep -rnE "$pattern" --include='*.rs' app crates tools 2> /dev/null \
      | grep -v -E '^[^:]*/tests/' || true)"
    if [ -n "$hits" ]; then
      while IFS= read -r hit; do
        echo "::error file=${hit%%:*}::a local copy of what ${crate} provides; use the crate and delete it: ${hit#*:}" >&2
      done <<< "$hits"
      found=1
    fi
  done <<< "$RETIRED"
  return "$found"
}

# self_test: proves a clean tree passes, and that each kind of finding fails.
self_test() {
  local dir crate
  dir="$(mktemp -d)"
  trap 'rm -rf "$dir"' RETURN
  mkdir -p "$dir/app/x/src" "$dir/app/x/tests/it" "$dir/crates" "$dir/tools"
  : > "$dir/Cargo.toml"
  for crate in "${ADOPTED[@]}"; do
    printf '%s = { version = "=1.2.3", default-features = false }\n' "$crate" >> "$dir/Cargo.toml"
  done
  printf 'pub struct Secret {\n    pub key: &'"'"'static str,\n}\n' > "$dir/app/x/src/form.rs"
  printf 'struct Readiness {\n    state: String,\n}\nstruct Lifecycle;\n' > "$dir/app/x/tests/it/mirror.rs"
  if ! check "$dir" 2> /dev/null; then
    echo "shared-crates: self-test: a clean tree was refused" >&2
    return 1
  fi
  local -a cases=(
    'pub struct Secret(SecretString);'
    'pub struct Held(pub secrecy::SecretString);'
    'pub(crate) fn read_secret(key: &str) {}'
    'pub trait HealthIndicator: Send {}'
    'pub struct Lifecycle(Arc<AtomicU8>);'
    'pub struct Reporter {'
    'pub struct UsageReport {'
    'pub struct TlsListener {'
    'pub async fn serve_until<L, F>('
    '        .layer(CatchPanicLayer::custom(caught))'
    'use tower_http::timeout::TimeoutLayer;'
    'pub struct Admission {'
    'pub struct SampleRatio(f64);'
    'pub enum Format {'
    'pub fn sampler(ratio: SampleRatio) -> Sampler {'
    'use opentelemetry_otlp::WithExportConfig as _;'
    '    std::panic::set_hook(Box::new(hook));'
    'pub fn install_hook() {'
    '    let subscriber = tracing_subscriber::fmt()'
    'pub fn same_origin(headers: &http::HeaderMap, origin: Option<&str>) -> bool {'
    'async fn shutdown_signal() {'
    'fn apply_override(table: &mut toml::Table, name: &str, raw: &str) {}'
  )
  local case
  for case in "${cases[@]}"; do
    printf '%s\n' "$case" > "$dir/app/x/src/copy.rs"
    if check "$dir" 2> /dev/null; then
      echo "shared-crates: self-test: missed a local copy: $case" >&2
      return 1
    fi
  done
  rm "$dir/app/x/src/copy.rs"
  sed -i.bak 's/"=1.2.3"/"1.2.3"/' "$dir/Cargo.toml"
  if check "$dir" 2> /dev/null; then
    echo "shared-crates: self-test: missed a pin that is not exact" >&2
    return 1
  fi
  echo "shared-crates: self-test passed"
}

case "${1-}" in
  --self-test) self_test ;;
  '')
    root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
    if check "$root"; then
      echo "shared-crates: FerroFED pins ${#ADOPTED[@]} FerroSYS crates exactly and keeps no copy of them"
    else
      echo "shared-crates: see ${SELF} for the rule" >&2
      exit 1
    fi
    ;;
  *)
    echo "usage: ${SELF} [--self-test]" >&2
    exit 2
    ;;
esac
