---
name: spec-pin-1-0-0
description: "The Federation Tier specification is vendored at its 1.0.1 erratum (untagged commit b4242c2, the owner's exception of 2026-11-02 to the tag-only pin, #1478, retired by #1480 when upstream tags it), after 1.0.0 (tag 1.0.0, commit 7dcf0f4, approved by the openEHR SEC 2026-10-12, #17); the reference implementation moves with it, now 9cb8606"
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

On 2026-10-01 the owner chose to vendor the release candidate and re-pin in
a dedicated issue when 1.0 shipped. The openEHR SEC approved 1.0.0 on
2026-10-12, and #17 moved the pin.

- Specification: <https://github.com/syntaric/openehr-federation-spec>,
  `antora.yml` version `1.0`, display `spec-version: 1.0.0`,
  `spec-status: Approved by the openEHR SEC`, `spec-date: 2026-10-12`;
  pinned tag `1.0.0`, commit `7dcf0f44ac122fe947b1e5ceba6bcbbb7721917d`.
  CC0 1.0. The rendered site serves it under `federation-aql/1.0/`, and the
  `0.9` path no longer resolves.
- What 1.0.0 changed against `7162d0c`: the declared version, the `$id` of
  both schemas (`federation-aql/1.0/_attachments/...`), §7a.2's
  `federation.spec_version`, now `"1.0"` (the first value that names the
  SEC-amended contract unambiguously), and the change record page
  `changes-from-2025-08-20.adoc` removed. No N, CP, § or anchor moved.
- Reference implementation: <https://github.com/syntaric/openehr-federation-ref>,
  pinned commit `92aff3cb1d8738ea0ce0e013b5a8fc2942438fd5` (2026-09-28).
  It has no release matching 1.0.0; its newest tag is `0.9.0`. Apache-2.0.
  Vendored whole as evidence, never an oracle and never a source of code
  (owner, 2026-10-01).

**1.0.1 (2026-11-02, #1478).** Upstream's `main` commit
`b4242c291b08990df718bbfaa1188e4865478e5a` ("release 1.0.1", 2026-11-01)
declares `spec-version: 1.0.1` and has no tag. The owner pinned it anyway,
an exception to the tag-only pin of this specification, because 1.0.1
corrects the routing the gateway implements: every request under a path
`ehr_id`, version reads and versioned writes included, is located by the
explicit target, the binding, the index, then the `creating_system_id` of a
path `OBJECT_VERSION_ID` (step 4), then for reads the ask-all probe; a write
located at a copy's holder is `409` and never redirected (§12.3, §12.4,
§12.5.1, N22, N23, N36, N41). `spec_version` stays `"1.0"` (§7a.2). The
reference implementation moved to `9cb8606d1fce457b6b7f3a42f703d7069686a413`,
which tracks 1.0.1. The `docs/VERSIONS.md` row names the newest tag, `1.0.0`,
so `scripts/checks/pin-freshness.sh` reports it stale when upstream tags
again; #1480 then moves the pin to the tag and drops the exception.

**Why:** a pin on a released, approved text keeps every citation checkable
and the `spec_version` the gateway reports true.

**How to apply:** cite `§`, `N#` and `CP-#` against the 1.0.0 text. A
citation made against 0.9.0 in a dated record stays as it was. A later
release is re-pinned the same way: move the tag and commit row in
`docs/VERSIONS.md`, re-run `scripts/vendor/federation-spec.sh`, diff the
pages and the CP table (which only appends), and reclassify every
obligation row of a page whose keyword lines changed.
