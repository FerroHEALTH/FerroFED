---
name: spec-pin-1-0-0
description: "The Federation Tier specification is vendored at its 1.0.0 release (tag 1.0.0, commit 7dcf0f4, approved by the openEHR SEC 2026-10-12), re-pinned from the 0.9.0 release candidate at 7162d0c by #17; the reference implementation stays at 92aff3c, whose newest tag is 0.9.0"
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

**Why:** a pin on a released, approved text keeps every citation checkable
and the `spec_version` the gateway reports true.

**How to apply:** cite `§`, `N#` and `CP-#` against the 1.0.0 text. A
citation made against 0.9.0 in a dated record stays as it was. A later
release is re-pinned the same way: move the tag and commit row in
`docs/VERSIONS.md`, re-run `scripts/vendor/federation-spec.sh`, diff the
pages and the CP table (which only appends), and reclassify every
obligation row of a page whose keyword lines changed.
