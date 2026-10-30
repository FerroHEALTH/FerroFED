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
  v0.0.12 does not keep this rule: it moves the issuer policy of `[auth]`
  to `[trust]`, removes every static bearer token and renames the
  console's `FERROFED_VIEWER__` overrides to `FERROFED_CONSOLE__` with no
  release that still reads the old keys. Its upgrade notes list each one.
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

Only the newest release is supported. When a new release is published,
every earlier release stops receiving fixes: there are no maintenance
branches, no long-term-support line and no backports. A security fix ships
as a patch release of the newest release, with the fix alone or the reason
a functional change ships with it stated in the notes, and a published release is never changed, so upgrade to the newest release
to get the fix; upgrading is free
([`SECURITY.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/SECURITY.md#supported-versions)).

## Upgrade notes by release

### To v0.0.15, from v0.0.14

- Move `server.public_url`, `server.replica` and
  `server.bindings_drain_timeout_ms` to a new `[gateway]` table, and rename
  `server.overload_retry_after_s` to `server.retry_after_s`. This release
  still reads each old key and names it in `config check` and the start-up
  log; FerroFED 0.0.16 refuses it. The `FERROFED__` overrides follow, as
  `FERROFED__GATEWAY__PUBLIC_URL` (#1408).
- Remove `base_path = "/"`, or write `base_path = ""`: the root is the
  empty path, and a `/` is refused. Give `[server.caller_rate]` both
  `requests_per_second` and `burst`, which no longer default (#1408).
- Move the OTLP keys into one `[telemetry.otlp]` table: `endpoint`,
  `sample_ratio`, `trust_roots_file`, `client_identity_file`, and `traces`
  and `metrics`, both on unless set. The old `[telemetry]` and `[metrics]`
  OTLP keys and `[telemetry] logged_query_parameters` are refused as
  unknown, and an inline `client_identity` is no longer read (#1397).
- A `[server.tls]` or `[metrics.tls]` table with `client_ca_file` needs
  `client_auth = "required"` beside it to keep requiring a client
  certificate. A listener whose clients speak only TLS 1.2 needs
  `min_version = "1.2"` (#1397).
- Point a liveness probe at `{base}/health/liveness`. `{base}/health`
  follows readiness now and answers `503` while the gateway boots or
  drains, and `{base}/health/readiness` lists `indicators` as an array. The
  example Kubernetes manifests probe the new routes (#1397, #1445).
- A probe, log rule or alert that matched the `ready` or `not ready` line of
  `ferrofed healthcheck` matches `healthy` and `unhealthy` instead (#1453).
- A client that read `message` from an error body outside the FHIR face
  reads `detail` of the RFC 9457 problem details document; `code` and
  `request_id` stay (#1412).
- A secret file loses only its trailing line ending: remove any space or
  tab that sits at either end of a secret by accident (#1397).
- A log alert on the panic line matches the target
  `ferrohealth_telemetry::panic` and its `file` and `line` fields (#1421).

### To v0.0.14, from v0.0.13

- Remove `[dev]` from your configuration: `[[dev.crossref]]` and
  `[[dev.consent_denied]]` are refused at load, naming the table. Resolve
  patients through a PIX Manager with `[[pixm.manager]]`, as the quickstart
  does with FerroPIX. With the `binding-nl` feature the gateway carries the
  IHE binding too, because the Dutch binding resolves patients through
  FerroPIX's PIXm Manager (#1377).
- A dispensation sent to `POST {fhir-base}/MedicationDispense` must carry
  its own business identifier, an `identifier` with a `system` and a
  `value`; one without it is refused `422` `required`, naming
  `MedicationDispense.identifier`. Have the pharmacy systems and the
  national connector that send dispensations through the gateway set it
  (#1346).
- A verified HTTP Basic credential of `[[auth.basic.users]]` is now kept
  for 300 seconds by default. Set `auth.basic.cache_ttl_seconds = 0` to
  check every request, as before (#1390).
- An access record writes its security labels once, on its
  `ehds-categories` and `origin` entities, and no `meta.security`. A
  repository rule or search that filtered on `_security` reads the
  entities' `securityLabel` instead (#1390).
- The quickstart runs FerroPIX beside the gateway, and its nodes run
  FerroEHR 5.5.0. If you edited `compose.yaml` or the quickstart's FerroPIX
  file, carry their renames over as the changelog lists them, and run
  `docker compose down -v` once before the first start (#1377, #1386).

### To v0.0.13, from v0.0.12

- The Kubernetes example moved to `deploy/kubernetes/base/`: apply it with
  `kubectl apply -k deploy/kubernetes/base` in place of
  `kubectl apply -f deploy/kubernetes/`. Its NetworkPolicy now admits only
  your reverse proxy's namespace (`ingress-nginx` until you edit it) and the
  console to the gateway port, so a client inside the cluster that called
  the gateway directly needs a peer of its own; the StatefulSet's
  `terminationGracePeriodSeconds` is 50 and its governing Service is the
  new `ferrofed-admin`. Kubernetes refuses a change of a StatefulSet's
  `serviceName`, so run `kubectl delete statefulset ferrofed
  --cascade=orphan` before you apply; the pods and their audit spool claims
  stay, and the new StatefulSet adopts them (#638).
- Each `[[fhir.receive]]` names, in `dependencies`, every package of its
  `package`'s dependency closure (each a `.tgz` or an unpacked directory),
  and the package of the MyHealth@EU value sets its profiles bind. A
  `[[fhir.receive]]` whose packages lack one a manifest names is refused at
  start and by `config check`, naming the missing package. For the HL7
  Europe Patient Summary, the book's receiving page lists the packages
  (#808).
- The quickstart `compose.yaml` now sends the usage report by default, as
  the release's compose file has since v0.0.12, with its instance id on the
  new `ferrofed-state` volume. Set `FERROFED__USAGE_REPORT__ENABLED=false`,
  and `FERROEHR__USAGE_REPORT__ENABLED=false` for the four nodes, in your
  shell or `.env` to keep it off (#1296).
- A bookmark of the operator console's `/configuration` now opens the
  Federation behaviour settings tab; the members are at
  `/configuration/members` (#1235).

### To v0.0.12, from v0.0.11

- The release's compose file and the Kubernetes example now set
  `FERROFED__USAGE_REPORT__STATE_DIR`, so a gateway started from them sends
  the installation report to FerroPULSE. Set
  `FERROFED__USAGE_REPORT__ENABLED=false` before you upgrade to keep it off
  (#1122).
- `[auth]` is now the `ferrohealth-smart` section every FerroHEALTH product
  reads (#1174). Move each issuer's FerroFED policy (`backend_clients`,
  `demographic_clients`, `client_tokens_act_for_professional`,
  `professional_issuing_authority`, `[auth.issuer.patient]`,
  `[auth.issuer.requester]`, `[auth.issuer.assurance]`,
  `[auth.issuer.national_contact_point]` and an introspected issuer's
  `introspection_endpoint`, `client_id` and `client_secret_file`) to a
  `[[trust.issuer]]` entry naming the same `issuer`, and `auth.mode`,
  `[auth.edge]` and `[auth.purpose_of_use]` to `trust.mode`, `[trust.edge]`
  and `[trust.purpose_of_use]`. Drop `jwks_uri` from each issuer: the key
  set is read from the issuer's RFC 8414 metadata, or from `jwks_file`.
  Replace each issuer's `operator_scope` with the one `auth.product_scope`,
  a string starting with `__` or a full URI you control. `auth.audience` is
  a list, and the clock skew, key set and timeout keys are
  `clock_skew_seconds`, `jwks_max_age_seconds`,
  `jwks_refresh_interval_seconds` and `timeout_ms`.
- Every static bearer token is gone (#1163). Replace `bearer_token` and
  `bearer_token_file` in a `[credentials."<endpoint>"]` section or a
  service's credentials with `user` and `password_file`, or with an `oauth`
  table, the client credentials grant of `ferrohealth-smart`; Mitz, the NVI
  and the terminology server take `user` and `password_file`. Replace
  `[metrics] scrape_token_file` with `scrape_users`, naming users of
  `[[auth.basic.users]]` whose Argon2id `password_hash_file` you write, and
  give Prometheus `basic_auth`. Replace `[pmir] feed_token_file` with
  `feed_users`, naming the Registry's own `[[auth.basic.users]]` user, and
  give the Registry its password.
- An `oauth2` section that asks for a plain client-credentials token, with
  no token exchange, no `DPoP` key, no binding to or authentication by the
  TLS client certificate and an assertion naming the token endpoint, is
  refused: move it to an `oauth` table, whose `private_key_jwt` signs with
  its own `private_key_file`.
- A log filter that names `ihe_iti::atna` (the audit spool and forwarder)
  names `ihe_atna::atna` instead; the events are the same.
- The operator console is now `ferrofed-console` (#1219). Pull
  `ghcr.io/ferrohealth/ferrofed-console` in place of
  `ghcr.io/ferrohealth/ferrofed-viewer`, rename every
  `FERROFED_VIEWER__<SECTION>__<KEY>` override to
  `FERROFED_CONSOLE__<SECTION>__<KEY>` and `FERROFED_VIEWER_CONFIG` to
  `FERROFED_CONSOLE_CONFIG`, and run the binary as `ferrofed-console`. The
  session and theme cookies are renamed, so every operator signs in once
  more after the upgrade.
- If you edited the `ferroehr-config` block of the quickstart's
  `compose.yaml`, FerroEHR 5.3.2 refuses the 4.3.9 Basic user keys at boot:
  rename `username` to `user`, give each user its `scopes` (the quickstart
  grants `user/composition-*.cruds`, `user/template-*.cruds`,
  `user/aql-*.cruds` and `__ferroehr.platform`), and move each user's
  `roles` to `[authz.rbac.basic_roles]` keyed by the user (#1226).
- Bookmarks of the operator console's `/members`, `/routing` and `/federation` pages answer `404`: use `/organisations`, a node's Routes tab under `/organisations/{org}/{node}/routes`, and `/self-description` (#1230). Each preference is now a cookie of its own (`ferrofed_console_theme`, `ferrofed_console_density`, `ferrofed_console_motion` and the others); the theme cookie is `ferrofed_console_theme` in place of 0.0.11's `ferrofed_viewer_theme`, so each operator chooses the theme once more. A deployment that builds the site bundle itself runs `cargo leptos build --split`, as `scripts/release/console-site.sh` does.

### To v0.0.11, from v0.0.10

- If the operator console's `[oidc] scopes` lists `offline_access`, remove
  it from the list and set `[oidc] offline_access = true` instead (#643).
  The console now refuses the scope in the list, and the new setting asks
  for it with `prompt=consent` so the console can refresh the operator's
  access token.
- Check the templates of every member that receives documents on the FHIR
  face (`[[fhir.receive]]`) before you upgrade (#714). The mapping now
  takes a composition's `category` from the template: its assumed value,
  its one admitted code, or `event` when the template leaves the category
  open. A template that admits several `COMPOSITION.category` codes and
  assumes none now refuses every received document of its category with a
  `422` (`MissingRequiredSuffix` on `<root>/category|code`), where `event`
  was taken before. Give the template's `category` an assumed value (AOM
  1.4 `assumed_value`) and upload it to the member again.
- `ferrofed_node_requests_total` carries a new label, `class`: `member`
  for a member's own answer or failure, `client-error` for a `4xx` it
  answered a routed request with (#982). Such a `4xx` now counts as
  `outcome="node-error"`, where it was `active`. The shipped
  `FerroFEDMemberFailing` rule leaves `class="client-error"` out, so it
  pages on the same member failures as before. A rule or a panel of your
  own that reads `outcome="node-error"` should leave it out too; take the
  rules and the dashboard this release attaches.

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
