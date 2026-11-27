<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Upgrading

**Audience:** operators. This page says how to move a gateway to a newer
release, what the current release asks you to change, and which configuration changes FerroFED makes without
notice and which it never does. [Rollback](rollback.md) says how to go back.
No specification governs this page: our own design.

## Before you upgrade

1. Read the release's **Upgrade notes**, the first section of its
   [changelog](https://github.com/FerroHEALTH/FerroFED/blob/main/CHANGELOG.md)
   entry. They list every change you must make: a key that became
   mandatory, a value that is now refused, a key renamed, a file to move.
   The notes of the current release are below.
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

### To v0.0.22, from v0.0.21

- Give the FHIR face a key for its stable ids: `fhir.id_key`, or
  `fhir.id_key_file`, 32 random bytes in base64url (for example
  `openssl rand 32 | basenc --base64url`). A configuration with `[fhir]` and
  no key is refused at start and by `config check`. Rotating the key changes
  every patient id the face gives
  ([Looking a patient up](../integrate/patient-summary.md#looking-a-patient-up),
  #1976).
- `[store]`: the gateway sends the libpq `options` startup parameter on
  every connection, which sets `statement_timeout`, `lock_timeout` and
  `idle_in_transaction_session_timeout` to `[store] timeout`. A connection
  pooler in front of the store must pass it on, or the gateway must connect
  to PostgreSQL itself ([FerroFED's own store](store.md), #2004).
- Name a terminology server in `[eehrxf.terminology]` where the FHIR face
  writes discharge reports: a written discharge report states the record's
  language only once that server answers that `eHDSILanguage` holds it, so
  without one each discharge report is listed unavailable with the reason
  ([Discharge reports](../integrate/discharge-reports.md), #1962).

### Earlier releases

The upgrade notes of every earlier release are the "Upgrade notes" of its
section in the
[changelog](https://github.com/FerroHEALTH/FerroFED/blob/main/CHANGELOG.md).
Move one release at a time, and run each release's `config check` over the
configuration before you start it.

### Unreleased

The pending notes are the `*.upgrade.md` files under
[`changelog.d/`](https://github.com/FerroHEALTH/FerroFED/tree/main/changelog.d),
which the next release lists under its Upgrade notes.

Check before upgrading: a receiving member's template that admits several
`COMPOSITION.category` codes and assumes none now refuses every received
document of its category (`MissingRequiredSuffix` on `<root>/category|code`),
where the composition took `event` before. Give the template's category an
assumed value ([The template's category](../integrate/receiving-a-document.md#the-templates-category)).
