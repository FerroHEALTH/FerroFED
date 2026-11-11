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
3. Back up the stored-query store when `[stored_queries]` is set, and
   FerroFED's own store when `[store]` is set: copy the `redb` file while no
   gateway has it open, or dump the `ferrofed` schema of the PostgreSQL
   database (`pg_dump --schema=ferrofed`). The new release may migrate a
   store, and an older release refuses a store a newer one migrated.
4. Run the new binary's `config check` over the configuration, with the
   upgrade notes applied. It reads every file and secret as `serve` does,
   binds no socket, and exits `78` naming each key it refuses. With the
   image:

   ```text
   docker run --rm --volume ./ferrofed.toml:/etc/ferrofed/ferrofed.toml:ro \
     --volume ./secrets:/run/secrets/ferrofed:ro \
     ghcr.io/ferrohealth/ferrofed:<new version> config check
   ```

   A key the new release renamed or removed is refused by name, with the
   key that replaces it, so fix each one the upgrade notes list before you
   roll out.

Then roll the new release out: `docker compose up --wait` with the new
`compose.yaml`, or a rolling update of the StatefulSet. Readiness answers
`503` until the new process has loaded the configuration and the store.

## Several replicas on one PostgreSQL store

A replica migrates FerroFED's own store ([`[store]`](store.md#migrations))
when it starts, under an advisory lock, so replicas starting together apply
each migration once. Once one replica of a release that brings a new
migration has started, a replica of the older release refuses its next
connection to the store ([Rollback](rollback.md#the-stored-query-store)).
When the upgrade notes name a new migration, upgrade every replica in one
rollout and expect the routes of the old replicas that need the store to
answer `500` until they are replaced. A release whose notes name none can be
rolled out replica by replica.

## The compatibility policy

FerroFED has not released 1.0. Until it does, any release can change the
configuration, and these rules bound how.

- **A change that stops a configuration the last release accepted is in
  the Upgrade notes.** A key made mandatory, a value now refused, a table
  moved: each is listed with what to set instead. CI runs the new build's
  `config check` over the example configuration the last release attached,
  and refuses a change it rejects unless an upgrade note is pending.
- **A key can be renamed or removed in any release.** Before 1.0 there is
  no deprecation release: the release that renames or removes a key stops
  reading the old one. It refuses the old key at start-up and in `config
  check`, in the file and as a `FERROFED__` override: a renamed or moved
  key naming the key that replaces it, and a key removed with nothing in its
  place by its name. Its upgrade notes give the step. That refusal lives
  for the one release that renames or removes the key; from the next
  release on, the key is refused as any unknown key is, and the upgrade
  notes of the release that removed it stay the record.
- **A new key is optional, or the Upgrade notes say otherwise.** A release
  that adds a key gives it a default that keeps the behaviour of the
  release before, unless the change is a security fix that cannot keep it.
- **An older release refuses a key it does not know.** Every table refuses
  unknown keys, so a gateway rolled back to an earlier release refuses a key
  added since. Remove the keys the newer release added before rolling back
  ([Rollback](rollback.md#the-configuration)).
- **The environment follows the file.** A `FERROFED__` override names the
  same key as the file, so a rename applies to it too, and the refusal
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

### To v0.0.19, from v0.0.18

The keys v0.0.18 refused by name, the outbound TLS keys outside a `tls`
sub-table, a node's `oauth2` and `fapi2` tables and `mtls_alias_hosts`, the
removed `[trust.issuer]` tables and `client_tokens_act_for_professional`,
and `[stored_queries] url` and `url_file`, are now refused as unknown keys.
Move them as the notes for v0.0.18 below say before you upgrade.

- Write the spans of time of `ferrohealth-smart` 0.15 as durations under
  keys that name no unit: on each `[[auth.issuer]]` `clock_skew`,
  `jwks_max_age`, `jwks_refresh_interval` and `timeout`;
  `introspection.cache_max_age`; `[auth.dpop]` `max_age` and `clock_skew`;
  on each `[credentials."<id>".credentials.oauth]` `refresh_margin`, `timeout` and
  `previous_key_overlap`; and `[onward.keys] max_onward_lifetime`, until
  now a bare number of seconds. The old keys are refused as unknown keys
  (#1799).
- Write `[auth] scopeless_subjects` entries as tables with `issuer` and
  `sub` (#1799).
- Move `[trust.issuer.patient] act_is_representative` to
  `act_represents_patient = true` on the `[[auth.issuer]]` of `kind =
  "patients"`. The old key refuses the start, naming its replacement. A
  patient token that carries an RFC 8693 `act` claim on an issuer without
  it is now refused as an invalid token (#1799).
- In the operator console, write `[oidc] timeout` and `[session]`
  `sign_in_timeout`, `idle_timeout` and `absolute_timeout` as durations,
  and move `[oidc] ca_file` to `[oidc.tls] trust_roots_file` (#1799).
- Move the `[server]` keys of `ferrohealth-server` 0.8 to their durations
  and sizes: `request_timeout`, `header_read_timeout`, `idle_timeout`,
  `drain_delay`, `shutdown_timeout`, `retry_after` (whole seconds),
  `body_limit`, `compression_min_size` and `[server.tls]`
  `reload_interval`, as in `request_timeout = "30s"`. The old keys
  (`request_timeout_ms` and the others) are refused as unknown keys
  (#1799).
- Move `[telemetry.otlp]` `client_identity_file` and `trust_roots_file`
  into `[telemetry.otlp.tls]`, which also takes `min_version` (#1799).
- Write every span of time and amount of data of FerroFED's own sections
  as a duration (`"30s"`, `"5min"`) or a size (`"16MiB"`) under a key that
  names no unit: `federation.per_node_timeout`, `overall_timeout`,
  `binding_ttl` and `max_node_answer_size`, `federation.localization.timeout`,
  `gateway.bindings_drain_timeout`, `signing.node_jwks_cache` and
  `rotation_overlap`, `emergency.window`, `config.poll` and `confirm`,
  `store.timeout` and `sweep_interval`, `fhir.document_url_lifetime`,
  `eehrxf.terminology.endpoint.timeout`, `pdqm.timeout`, `pmir.timeout` and
  `check_interval`, `nl_gf.mitz.timeout`, `registry.mcsd.refresh_interval`,
  `deadline` and `max_size`, `audit.repository.spool_max_size` and
  `spool_write_timeout`, `audit.repository.endpoint.timeout` and
  `retry.max_backoff`, and
  `xcpd.audit_repository.spool_max_size`, `spool_write_timeout`,
  `connect_timeout`, `send_timeout` and `retry_max`, with the
  `FERROFED__` overrides to match. Each old key (`per_node_timeout_ms`,
  `window_s`, `spool_max_bytes` and the others) refuses the start and
  `config check`, naming its replacement. The operator console writes
  `gateway.timeout` in place of `gateway.timeout_ms` (#1800).
- Move the address, timeouts, TLS and credential of `[audit.repository]`
  and `[eehrxf.terminology]` into their new `endpoint` table,
  `ferrohealth-smart`'s outbound table. `[audit.repository]` keeps
  `hostname`, `source_id`, `enterprise_site` and the `spool_*` keys, and
  `[audit.repository.endpoint]` takes `url`, `timeout` (from `timeout_ms`),
  `retry.max_backoff` (from `retry_max_ms`, the wait between two delivery
  rounds) and `tls`. `[eehrxf.terminology.endpoint]` takes `url`, `timeout`
  (from `timeout_ms`), `tls` and `credentials`; one call is tried once
  unless `retry.max_attempts` says more. Each old key refuses the start and
  `config check`, naming its replacement (#1805).
- Move a node's `user`, `password`, `password_file`, `oauth` and `onward`
  from `[credentials."<id>"]` into its nested
  `[credentials."<id>".credentials]` table, `ferrohealth-smart`'s
  credentials table, with the `FERROFED__CREDENTIALS__<ID>__CREDENTIALS__`
  overrides to match. `tls`, `nuts` and `onward_organisations` stay in the
  section. Each old key refuses the start and `config check`, naming its
  replacement. The operator console's credentials form writes the nested
  table (#1806).
- Move the NVI's Nuts grant from `[nl_gf.nvi.credentials.nuts]` to
  `[nl_gf.nvi.nuts]`; `[nl_gf.nvi.credentials]` is `ferrohealth-smart`'s
  credentials table, which refuses `nuts` as an unknown key (#1806).
- Move the address, TLS and credential of every identity, localization and
  consent service into its `endpoint` table, `ferrohealth-smart`'s outbound
  table, as `[audit.repository]` and `[eehrxf.terminology]` moved: `url`,
  `tls` and `credentials` of `[[pixm.manager]]`, `[pdqm]`, `[pmir]`,
  `[registry.mcsd]`, `[nl_gf.nvi]` and `[nl_gf.mitz]` go to
  `[<table>.endpoint]`, and the `timeout` of `[pdqm]`, `[pmir]` and
  `[nl_gf.mitz]` (from `timeout_ms`) to `endpoint.timeout`. Each
  `[[xcpd.gateway]]` takes its own `[xcpd.gateway.endpoint]` with its `url`,
  and the shared `[xcpd.tls]` moves into the `tls` of each gateway's
  endpoint table. A table you leave partly written keeps its defaults: one
  try per call, `1s` for PDQm and Mitz, `5s` for PMIR and `10s` for the
  others. The NVI's Nuts grant stays in `[nl_gf.nvi.nuts]`. Each old key
  refuses the start and `config check`, naming its replacement (#1815).
- The release asset `compose.yaml` is now the self-contained quickstart. The
  gateway-only compose file of the production guide, which carried that name,
  is attached as `gateway-compose.yaml`: an operator who downloads it by name
  fetches `gateway-compose.yaml` and saves it as `compose.yaml` beside
  `ferrofed.toml` and `registry.toml`, as Install and run shows
  ([#1849](https://github.com/FerroHEALTH/FerroFED/issues/1849)).

### To v0.0.18, from v0.0.16

v0.0.17 was tagged and never released; these are its notes, which
v0.0.18 ships.

- Have a national contact point's authorization server write the format of
  the HP Identity Assertion's `NameID` as `ehdsi.assertion.name_id_format`,
  one of the three formats eHDSI SAML Profile 10.0.0 §2.1 allows.
  `ferrohealth-smart` 0.9.4 refuses a contact-point token without it, as it
  refuses one without the `NameID`
  ([Authentication](../operate/authentication.md), #1615).
- Run `docker compose down -v` once before you start the quickstart on
  this release. FerroEHR 5.6.0 starts its audit store empty and refuses one
  that holds records an earlier release wrote, so the nodes do not start on
  a `ferroehr-pgdata` volume that FerroEHR 5.5.0 wrote (#1617).
- The store applies migration 2, which adds `ferrofed.emergency_access`, at
  the first start. A deployment that declares a national contact point and
  names its emergency purpose in `[[access_log.emergency_purpose]]` now
  denies a professional's sixth emergency access in 24 hours; set
  `[emergency]` to tighten the limit, never to loosen it (#1631).
- `[trust.issuer.national_contact_point]` is removed, and a configuration
  that still sets it is refused, naming its successor. Declare the contact
  point as an `[[auth.issuer]]` of `kind = "federation"` with
  `sender_constraint = "required"`, an `[[auth.federation]]` table with
  `contact_point = true`, `mapping = "ehdsi"`, its `countries` and the
  constant `professional.country_code`,
  `professional.issuing_authority_name` and
  `provider.issuing_authority_name`, and a `[[federation.contact_point]]`
  naming the issuer, with `correlation_header` and `additional_countries`
  moved there. Have the contact point's authorization server write the
  assertion's attributes under `ehdsi`, the TRC patient as
  `ehdsi.patient.id`, and bind the token to the connector's client
  certificate ([National contact point](../integrate/national-contact-point.md),
  #1632).
- With `[stored_queries] backend = "postgres"`, move `url` or `url_file`
  from `[stored_queries]` to a new `[store]` table, pointed at the database
  the backend uses now, and set `sslmode=require` in the connection string,
  with `ca_file` when the database's CA is not one of the platform's roots.
  This release refuses `[stored_queries] url` and `url_file` at start-up and
  in `config check`, naming each, and a `[store]` without
  `sslmode=require` under every profile. The gateway takes over the
  existing table with every definition it holds
  ([FerroFED's own store](../operate/store.md#moving-from-stored_queries-url),
  #1636).
- A gateway with a national contact point (`[[federation.contact_point]]`)
  needs `[store]` too: give it a PostgreSQL database and role of its own,
  never a CDR's. Without it this release refuses to start, naming
  `federation.contact_point[0]` and `[store]` (#1636).
- A binary you build yourself no longer takes `--features postgres`; drop
  the flag (#1636).
- Write each issuer as an `[[auth.issuer]]` table with its `kind`:
  `persons` for your professionals, `patients` for an issuer whose
  `patient/` grants `[trust.issuer.patient]` binds, `machines` for systems
  under the client credentials grant alone, or `federation` with its
  `[[auth.federation]]` table. Move `audience`, `algorithms`,
  `clock_skew_seconds`, `jwks_max_age_seconds`,
  `jwks_refresh_interval_seconds`, `jwks_file`, `timeout_ms` and
  `role_claims` from `[auth]` onto every issuer table they held for, and
  name the issuer's `profile` when it is not a standards-only server
  (`entra`, `keycloak`, `okta` or `nl`). The start refuses a section-level
  key and an issuer without `kind`, naming each (#1682).
- On each `persons` issuer, list your own applications in
  `first_party_clients`, and a third-party client or an RFC 8693 actor in
  `delegates`; a person's token through any other client is refused
  (#1682).
- Move an introspected issuer from `[[trust.issuer]]` to its own
  `[[auth.issuer]]` with `mode = "introspection"` and an
  `[auth.issuer.introspection]` table: `endpoint`, and the client
  authentication in `[auth.issuer.introspection.client]` (`client_id`,
  `client_secret` or `client_secret_file`). Keep its `[[trust.issuer]]`
  policy, without the introspection keys (#1682).
- Rename `[metrics] scrape_users` to `scrape_clients` and `[pmir]
  feed_users` to `feed_clients`, naming the client ids of your Prometheus
  scraper and your PMIR Registry at an `[[auth.issuer]]` the gateway
  trusts, and remove every `[[auth.basic.users]]` entry: the gateway admits
  no Basic caller, and a configuration that still writes one refuses to
  start, naming the user. Give the scraper an `oauth2` client-credentials
  setup in its scrape job, and the Registry a client-credentials grant at
  that issuer (#1704).
- Move `[trust.issuer.assurance]` to the issuer's `[auth.issuer.assurance]`
  table (`claim`, `minimum`, and a `levels` map from each claim value to
  `low`, `substantial` or `high`), move `[trust.issuer.requester]` to the
  `nl.professional_number`, `nl.role`, `nl.ura` and `nl.organisation_type`
  fields of the caller issuer's `[[auth.federation]]` table, and remove
  `client_tokens_act_for_professional`. Each removed key refuses the start,
  naming its replacement (#1704).
- Remove any user name or password from an `[[auth.issuer]]` URL, and keep
  only public keys in an issuer's `jwks_file`: `ferrohealth-smart` 0.13.0
  refuses the start on either (#1748).
- A national contact point's authorization server that answers the exchange
  profile authenticates the national connector before it reads an
  assertion, keeps one record of admitted exchanges that every replica
  shares, and refuses a replay `invalid_grant`
  ([National contact points](../integrate/national-contact-point.md), #1748).
- Move every `[credentials."<id>".oauth2]` and `[credentials."<id>".fapi2]`
  table to `[credentials."<id>".oauth]`, and the section's
  `mtls_alias_hosts` into it. This release refuses each removed key at
  start-up and in `config check`, naming the `oauth` key that replaces it
  ([Onward credentials](../operate/onward-credentials.md), #1749):
  - The grant's key moves from `[signing]` to the `oauth` table's own
    `private_key_file`, with `previous_private_key_file` and
    `next_private_key_file` for a rotation. The gateway publishes it in its
    JWK Set beside the `[signing]` keys. Pointing `private_key_file` at
    the `[signing]` key file keeps the `kid` the node's authorization
    server knows; a new key needs its `kid` registered there, or the JWK
    Set fetched again, before you switch.
  - `grant = "token_exchange"` becomes an `onward` table beside the `oauth`
    table, with `kind = "persons"`, the exchanged tokens' `issuer`,
    `audiences` and `indicators` naming the node in place of `resource`,
    `actor_assertion = true`, and `[onward.keys] max_onward_lifetime`. The
    node's authorization server must issue a JWT access token that names
    that issuer, an `aud` among `audiences`, and the gateway as `act` or
    client; an opaque exchanged token now fails the node.
  - A `fapi2` table becomes an `oauth` table with
    `security_profile = "fapi2"`, `client_auth = "private_key_jwt"`,
    `assertion_audience = "issuer"`, `issuer`, and `client_key_file` and
    `previous_client_key_file` renamed `private_key_file` and
    `previous_private_key_file`.
  - `tls_client_certificate_bound_access_tokens = true` becomes
    `certificate_bound = true`. A grant that authenticates by, or binds its
    tokens to, the TLS client certificate presents the node section's
    `tls` identity; `client_certificate_file`, `client_key_file` and
    `ca_file` in its `oauth` table are refused.
- Give every node's `oauth` grant `client_auth = "private_key_jwt"`,
  `"tls_client_auth"` or `"self_signed_tls_client_auth"`. A node's grant
  that names `client_secret_basic`, `client_secret_post`, `client_secret`
  or `client_secret_file`, or leaves `client_auth` unset, is refused,
  naming the key (§13.1, N25). Identity service grants keep their client
  secret (#1749).
- Remove `signing.assertion_lifetime_s`: no `[signing]` key signs a client
  assertion any more, and every assertion lives 300 seconds. A file that
  sets it is refused at start-up, naming it. `signing.rotation_overlap_s`
  must be at least 300 plus `signing.node_jwks_cache_s` (#1749).
- Move each table's outbound TLS keys, `client_identity`,
  `client_identity_file` and `trust_roots_file`, into its `tls` sub-table:
  `[[pixm.manager]]` to `[pixm.manager.tls]`, and likewise `[pdqm.tls]`,
  `[pmir.tls]`, `[registry.mcsd.tls]`, `[xcpd.tls]`, `[nl_gf.nvi.tls]`,
  `[nl_gf.mitz.tls]`, `[audit.repository.tls]`, `[eehrxf.terminology.tls]`
  and `[credentials."<endpoint id>".tls]`. This release refuses a key left
  on its table at start-up and in `config check`, naming the `tls` key that
  replaces it
  ([Mutual TLS to the identity services](../operate/configuration.md#mutual-tls-to-the-identity-services),
  #1750).
- The gateway speaks TLS 1.3 alone to every outbound service unless the
  table's `tls` sub-table lowers it with `min_version = "1.2"`. Set it on
  `[xcpd.tls]` when a MyHealth@EU responding gateway speaks only TLS 1.2,
  the minimum eHDSI Section II Security Services 10.0.0 §5.9.1 and eHDSI
  Cryptographic Algorithms 9.0.0 admit between NCPeH, and on any other
  table whose service speaks no TLS 1.3
  ([A MyHealth@EU route](../operate/localization.md#a-myhealtheu-route),
  #1750).
- Move `client_identity`, `client_identity_file` and `trust_roots_file`
  of `[xcpd.audit_repository]` into its `tls` sub-table,
  `[xcpd.audit_repository.tls]`, as every other outbound table took them
  with #1750; the sub-table also takes `min_version`, `"1.3"` unless
  lowered to `"1.2"`. This release refuses a key left on the table at
  start-up and in `config check`, naming the `tls` key that replaces it
  ([The audit repository](../operate/localization.md#the-audit-repository), #1766).

### To v0.0.16, from v0.0.15

- Write `server.public_url`, `server.replica` and
  `server.bindings_drain_timeout_ms` as `gateway.public_url`,
  `gateway.replica` and `gateway.bindings_drain_timeout_ms`, and
  `server.overload_retry_after_s` as `server.retry_after_s`, with the
  `FERROFED__` overrides to match. This release refuses each old key at
  start-up and in `config check`, naming it, as it refuses any unknown key
  (#1490).
- Remove `[server] site_root` and its `FERROFED_CONSOLE__SERVER__SITE_ROOT`
  override from the operator console's configuration: the console serves no
  site bundle, and this release refuses the key at start-up and in
  `config check`, naming it (#1512).
- Nothing in the operator console's sign-in configuration has to change
  (#1504). The `[oidc]` endpoints are now optional: remove
  `authorization_endpoint`, `token_endpoint` and `jwks_uri` to have the
  console read them from `issuer`'s discovery document when it starts,
  which then needs the provider reachable at start. A reverse proxy in front
  of the console passes `/session` through to it.

### To v0.0.15, from v0.0.14

- Move `server.public_url`, `server.replica` and
  `server.bindings_drain_timeout_ms` to a new `[gateway]` table, and rename
  `server.overload_retry_after_s` to `server.retry_after_s`. That release
  still read each old key and named it in `config check` and the start-up
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
