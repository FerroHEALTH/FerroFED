<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Adding a country

The Federation Tier's core names no national service. A deployment fills
the roles the specification names with the services of its region:
resolution (§5.2), localization (§14), the consent pre-filter (§13.2.1),
addressing (§8, §15) and the authentication to each node (§13). FerroFED
holds each region's services in one *binding*: one module of the server, one
Cargo feature, and one implementation of the `Binding` trait. The IHE binding
of Annex A and the Dutch binding of Annex B are built this way, and a new
country follows the same three steps.

Start from evidence. The research on
[#488](https://github.com/FerroHEALTH/FerroFED/issues/488) maps the five roles
for several countries and names which existing crates cover each one. A
role that `ihe-iti` or the onward OAuth 2.0 grants already cover needs no new
code, only configuration.

## 1. A specification crate

The wire of each national service goes in a library crate under `crates/`,
named for the specification it implements and never for FerroFED, with one
feature per service. It depends on nothing in FerroFED, so another product
can use it as it is. Vendor the specification it implements first, with a
`scripts/vendor/*.sh` fetch script, a `PROVENANCE.md` and a row in
`docs/VERSIONS.md`, and hold the crate to it with tests at the wire.

## 2. The adapters and the binding module

The adapters that turn the crate's clients into the role traits
(`Localizer`, `Resolver`, `ConsentPrefilter`, `Demographics`) sit in
`app/ferrofed-identity`, beside the IHE and Dutch adapters.

The binding itself is one module, `app/ferrofed-server/src/binding/<name>/`,
with a unit struct that implements `Binding`. It declares:

- **its sections:** each key it reads, and whether a registry reload applies
  a change (`Reload::Applies`) or the change takes a restart
  (`Reload::Restart`);
- **its resolve:** each section read and checked into its settings, refused
  under the key at fault, with every secret read from its `_file` sibling;
- **its offers:** the roles its configured sections fill, read without
  building anything, so the server can hold the role rules first;
- **one builder per role it fills:** `resolver`, `demographics`, `localizer`
  and `prefilter`, each built over the registry snapshot;
- **its transport sites:** every URL a patient identifier or a credential
  travels to, held to `https` outside the development profile;
- **its reload rules:** `effective` keeps the boot's value of what takes a
  restart, and `needs_restart` names each such key that changed;
- **its self-description:** the `mode` each role it builds carries, which
  `OPTIONS {base}/` declares as `localization.mode` or
  `consent.prefilter`;
- **its health indicators and instruments**, when it records through
  something of its own, such as an audit trail.

An onward credential kind of the region, such as the Nuts grant of the Dutch
binding, is an `OnwardGrant` the binding resolves from its table under
`[credentials."<endpoint id>"]`.

The server holds the role rules for every binding: at most one resolver, one
consent pre-filter and one localizer of a binding's own. Two bindings that
both fill one of those roles are refused with one error naming both sections,
so a new country adds no pairwise check.

## 3. One feature line

Register the binding in four places, each under its feature:

- the feature in `app/ferrofed-server/Cargo.toml`, as `binding-<name>`, with
  the optional dependencies it brings in, and in `default` when the released
  binary should carry it;
- the module and its entry in the list of compiled bindings in
  `src/binding/mod.rs`;
- its section in `Config` and its resolved settings in `Settings`, a field
  each, so the configuration refuses the section in a build without the
  feature;
- its process state, instruments or registry source in
  `src/binding/process.rs`, when it has any.

Gate its tests under `tests/it/` with the same feature. The
`features (cargo-hack)` job of CI lints the server with each feature alone,
so a binding that only builds beside another one fails there.

## What a binding does not do

A binding never adds a branch for a node, a vendor or a region to the core,
and never lets a patient identifier reach a node in any carrier (§5.4, N33).
A role the specification leaves to the node, such as the consent decision
itself (N27), stays with the node; the binding's pre-filter only narrows who
is asked.
