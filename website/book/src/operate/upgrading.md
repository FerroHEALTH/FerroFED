<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Upgrading

This page says how to move a gateway to a newer release, what each release
asks you to change, and which configuration changes FerroFED makes without
notice and which it never does. [Rollback](rollback.md) says how to go back.
No specification governs this page: our own design.

## Before you upgrade

1. Read the release's **Upgrade notes**, the first section of its
   [changelog](https://github.com/FerroHEALTH/FerroFED/blob/main/CHANGELOG.md)
   entry. They list every change you must make: a key that became
   mandatory, a value that is now refused, a key renamed, a file to move.
   The notes for the releases so far are below.
2. Keep a copy of the configuration you run now, `ferrofed.toml` and the
   registry document, and note the image digest or binary version. A
   rollback needs both.
3. Back up the stored-query store when `[stored_queries]` is set: copy the
   `redb` file while no gateway has it open, or dump the `ferrofed` schema
   of the PostgreSQL database (`pg_dump --schema=ferrofed`). The new release
   may migrate the store, and an older release refuses a store a newer one
   migrated.
4. Run the new binary's `config check` over the configuration, with the
   upgrade notes applied. It reads every file and secret as `serve` does,
   binds no socket, and exits `78` naming each key it refuses. With the
   image:

   ```text
   docker run --rm --volume ./ferrofed.toml:/etc/ferrofed/ferrofed.toml:ro \
     --volume ./secrets:/run/secrets/ferrofed:ro \
     ghcr.io/ferrohealth/ferrofed:<new version> config check
   ```

   The warnings it prints name every deprecated key you still set, and the
   release that will refuse it.

Then roll the new release out: `docker compose up --wait` with the new
`compose.yaml`, or a rolling update of the StatefulSet. Readiness answers
`503` until the new process has loaded the configuration and the store.

## Several replicas on one PostgreSQL store

A replica migrates the shared stored-query store when it starts. Once one
replica of a release that brings a new schema version has started, a
replica of the older release refuses its next connection to the store
([Rollback](rollback.md#the-stored-query-store)). When the upgrade notes
name a new schema version, upgrade every replica in one rollout and expect
the stored-query routes of the old replicas to answer `500` until they are
replaced. A release whose notes name none can be rolled out replica by
replica.

## The compatibility policy

FerroFED is in its `0.0.x` series, and each release can change the
configuration. These rules bound how.

- **A change that stops a configuration the last release accepted is in
  the Upgrade notes.** A key made mandatory, a value now refused, a table
  moved: each is listed with what to set instead. CI runs the new build's
  `config check` over the example configuration the last release attached,
  and refuses a change it rejects unless an upgrade note is pending.
- **A key is never renamed or removed at once.** For at least one release
  the old key is still read, under its new name, and `config check` and the
  start-up log warn that it is deprecated and name the release that will
  refuse it. Setting the old and the new key together is refused, so the
  two cannot disagree. The Upgrade notes name the rename in the release
  that deprecates the key and again in the release that refuses it.
- **A new key is optional, or the Upgrade notes say otherwise.** A release
  that adds a key gives it a default that keeps the behaviour of the
  release before, unless the change is a security fix that cannot keep it.
- **An older release refuses a key it does not know.** Every table refuses
  unknown keys, so a gateway rolled back to an earlier release refuses a key
  added since. Remove the keys the newer release added before rolling back
  ([Rollback](rollback.md#the-configuration)).
- **The environment follows the file.** A `FERROFED__` override names the
  same key as the file, so a rename applies to it too, and the warning
  names the dotted key.
- **Stored data migrates forward only.** A stored-query store records its
  schema version, is migrated forward in order when the gateway starts, and
  is never migrated back. A release refuses to start over a store a newer
  release migrated.

A security fix ships as its own patch release on each supported line, with
no feature changes in it. A line is the releases that share a
`<major>.<minor>` version, and it is supported while a release on it is
within its support period. A published release is never changed, so move
to the security patch release of your line to get the fix
([`SECURITY.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/SECURITY.md#supported-versions)).

## Upgrade notes by release

### To v0.0.10, from v0.0.9

- A gateway with a registry outside the development profile must set
  `[audit] destination` (#623, #693), and it must be `"repository"` with an
  `[audit.repository]` table: `"log"` is refused there, because it records no
  caller and no patient and so cannot hold the access log Regulation (EU)
  2025/327 Annex II 3.2 asks for. Point the gateway at your Audit Record
  Repository, with `spool_dir` on durable storage, before you upgrade.
- Set `server.shutdown_timeout_ms` to at least `server.request_timeout_ms`,
  or remove it so it takes the request timeout (#625). `config check` and
  `serve` now refuse a shorter drain, and the v0.0.9 example configuration
  set 10000 beside a request timeout of 30000. Then give the container
  longer to stop than `server.drain_delay_ms` plus the drain: the examples
  use `stop_grace_period: 40s` in Compose and
  `terminationGracePeriodSeconds: 45` in Kubernetes.
- If you run the Kubernetes example, move from `deployment.yaml` to
  `statefulset.yaml` (#628). Replace the `encrypted` storage class with
  your own. Wait until the dependency report, `GET
  {base}/operator/dependencies` with an operator token (#751), reads `up` for
  the audit repository, so the spool on the old `emptyDir` is drained, then
  delete the Deployment and apply the StatefulSet.
- The listeners need no change: unset, `[server.tls]` and `[metrics.tls]`
  leave them on plain HTTP as before (#632). Once you set either, remove it
  before rolling back to an earlier release, which refuses it as an unknown
  key.
- If `[metrics] listen` names an address other than a loopback one outside
  `profile = "development"`, set `[metrics] scrape_token_file` to a file
  holding a scrape token and give your Prometheus the same token with
  `authorization.credentials_file`, or set `[metrics.tls] client_ca_file`
  (#635). Without either, `serve` and `config check` refuse the
  configuration. The Kubernetes example reads the token from the
  `metrics-scrape-token` key of the `ferrofed-secrets` Secret.
- To run the admin listener's stored-query distribution outside the
  development profile, from the gateway's host or another, set
  `operator_scope` on the `[[auth.issuer]]` your operators use and send a
  token carrying it (#629, #635). Without one it answers
  `401 unauthenticated`, from a loopback peer as from any other, where a
  loopback peer was served before.
- If a backend client reads patient data through the gateway with a
  `system/` scope or a token whose `sub` is its `client_id`, set
  `client_tokens_act_for_professional = true` on its `[[auth.issuer]]` and
  have its tokens name the professional it acts for in
  `extensions.ihe_iua.national_provider_identifier` (#661). Until both
  hold, such a request is answered `401 natural-person-required`. Then set
  `[auth.issuer.assurance]` on each issuer whose tokens reach patient data;
  without it no assurance level is required, and `config check` notes the
  issuer.
- A document received on the FHIR face whose codings carry no `display` is
  now refused with a `422` unless `[eehrxf.terminology]` names a terminology
  server to look the displays up (#687). Senders that omit displays need the
  server configured.
- Read the dependency report at `GET {base}/operator/dependencies` with a
  token that carries your issuer's `operator_scope` (#751). `GET
  {base}/health/dependencies` is no longer served and answers `404`, so move
  every monitor that read it, and give it an operator token. `ferrofed report`
  reads the report with `--operator-token-file`. A proxy rule that kept
  `/health/dependencies` inside can go; the health family left open,
  `{base}/health` and `{base}/health/readiness`, names no member.
- Move any stored-query definition you hold in the namespace
  `eu.ferrofed.eehrxf`, or a namespace under it, to a namespace of your own
  before you upgrade (#776). The gateway now reserves that namespace for its
  own read-only queries: a `redb` or PostgreSQL store, or a definition file,
  holding one there refuses the start, and `config check` refuses such a
  file.
- If you wrote an `[access_log]` map on a development build, rewrite its
  categories with the codes of HL7 Europe's
  `EEHRxFDocumentPriorityCategoryCS` (#824), which are case-sensitive:
  `patient-summary` becomes `Patient-Summaries`, `electronic-prescription`
  `Electronic-Prescriptions`, `electronic-dispensation`
  `Electronic-Dispensations`, `medical-imaging` `Medical-Imaging`,
  `medical-test-result` `Laboratory-Reports` and `discharge-report`
  `Discharge-Reports`, in `[access_log.templates]`,
  `[access_log.archetypes]` and the keys of
  `[access_log.retention.categories]`. Declare each national category in
  `national_categories` as `<system>|<code>`, the absolute URI of your
  Member State's code system (or one you control) and its code, and name it
  the same way in the map and the retention table. The old spellings and a
  national code without a system are refused when the configuration
  loads. A search of your Audit Record Repository for a category matches
  the `ehds-category` detail `<system>|<code>`, never the bare code.
- The registry document refuses an `[[organisation]]` that manages no
  `[[endpoint]]` (§8.1: "an Organization manages 1..* Endpoints"), so
  `config check` and the gateway's start fail on it, naming the
  organisation (#874). Give the organisation an endpoint it manages, or
  remove it. A registry read from an mCSD directory or in the FHIR form
  keeps such an organisation, and selecting it still answers `404` with
  `no-destination`.

### To v0.0.9, from v0.0.8

- If you run the operator console with `[session] secure_cookie = false`,
  either set it to `true` or make `[oidc] redirect_uri` an `http` URL on a
  loopback host: the console refuses any other combination (#615).
- A stored-query store that holds a definition naming its patient by a
  literal, written past the gateway by a restore or a manual insert, refuses
  the start (#586). Remove that row, or the file, and store the definition
  again with a `$parameter`.
- The released binary and image carry both bindings, as before. A build of
  your own with `--no-default-features` needs `--features binding-ihe` or
  `binding-nl` for the sections it uses, or refuses them as unknown keys
  (#489).
- The `configuration resolved` log line no longer carries the registry
  directory, PIX Manager, PMIR and PDQm fields; they are on the IHE
  binding's own `binding configured` line (#489). Update a log query that
  reads them.

### To v0.0.8, from v0.0.7

- **Every caller authenticates.** A gateway that federates refuses to start
  without an `[auth]` table that trusts at least one issuer
  (`[[auth.issuer]]` and `auth.audience`), and every request to the
  ITS-REST surface and `OPTIONS {base}/` needs an access token from that
  issuer with a SMART on openEHR scope and a purpose of use, unless
  `auth.purpose_of_use.required = false` (#80;
  [Client authentication](authentication.md)). Issue tokens to your clients
  before you upgrade.
- **`[signing]` is mandatory** whenever a registry is configured, by
  `registry.document` or `[registry.mcsd]`: add a P-384 or P-256 signing key
  and the `jwks_uri` it is published at, and make
  `{base}/.well-known/jwks.json` reachable from the nodes' authorization
  servers ([What a node is told about the caller](caller-token.md)).
- **`[audit] destination`** is mandatory for a production gateway with
  `[pixm]`, `[registry.mcsd]` or `[pmir]`. Set `"log"` to keep the
  behaviour of v0.0.7 while you add an Audit Record Repository; `"off"` is
  refused outside development (#486; [The audit trail](audit.md)).
- **`[xcpd] audit`** is mandatory: `"log"`, `"repository"`, or `"off"`
  under the development profile alone (#410). With `"repository"`,
  `[xcpd.audit_repository] spool_dir` is mandatory outside development and
  `[xcpd] home_community` is mandatory.
- The ADMIN API under `{base}/v1/admin/` answers `403
  operation-refused`, where it answered `501`.

### Unreleased

The pending notes are the `*.upgrade.md` files under
[`changelog.d/`](https://github.com/FerroHEALTH/FerroFED/tree/main/changelog.d),
which the next release lists under its Upgrade notes.

Check before upgrading: a receiving member's template that admits several
`COMPOSITION.category` codes and assumes none now refuses every received
document of its category (`MissingRequiredSuffix` on `<root>/category|code`),
where the composition took `event` before. Give the template's category an
assumed value ([The template's category](../integrate/patient-summary.md#the-templates-category)).
