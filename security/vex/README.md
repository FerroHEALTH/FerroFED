<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# VEX statements

Vulnerability Exploitability eXchange documents, in
[OpenVEX](https://github.com/openvex/spec/blob/main/OPENVEX-SPEC.md) format,
stating whether a finding affects what FerroFED ships: the `ferrofed` binary
(`pkg:cargo/ferrofed-server`), the gateway image (`pkg:oci/ferrofed`) and the
operator console image (`pkg:oci/ferrofed-console`).

A VEX document is the machine-readable form of an argument. Each statement
names the vulnerability, the product, a `status`, a `justification` from the
OpenVEX vocabulary, and an `impact_statement` a reader can check. An ignore
list records the decision without the reasoning, which nobody can re-evaluate
later.

## Rules

- **`not_affected` needs a justification from the OpenVEX vocabulary**, and
  the `impact_statement` says concretely why the code is unreachable in
  FerroFED's use. "Low risk" is not a justification.
- **A finding that can be fixed is fixed.** A statement is for a finding
  whose fix belongs to someone else: an unmaintained crate with no successor,
  or a package of the base image with no fixed version.
- **Re-check on every base-image bump.** The images are
  `gcr.io/distroless/static-debian13`, which holds no shell, no libc and no
  package manager, only a few data packages (`base-files`, `netbase`,
  `tzdata`, `ca-certificates`) beside the static musl binary. A statement
  about a package of the base goes when the package changes.
- **The scanners read these files** (`trivy --vex`), so a statement that
  stops being true stops hiding anything: the finding returns and the scan
  fails. An `affected` statement leaves its finding visible with its action
  statement beside it.
- **Every finding a release ships carries a statement.** The release scans
  each image and binary with no severity floor and unfixed findings
  included, and `scripts/checks/vex-coverage.sh` refuses the release while a
  finding has no statement that judges it: `not_affected` with a
  `justification` or `impact_statement`, `affected` with an
  `action_statement`, or `fixed`. A statement matches a finding by its
  `vulnerability.name` or an `aliases` entry, for the product scanned.

## Documents

| File | Subject | Authored |
|---|---|---|
| `rust-advisories.openvex.json` | The Rust dependency advisories the advisory gate accepts. Each statement carries a `ferrofed:reachability` block (our own extension; OpenVEX defines none) naming the crates the affected package is reached through. | **generated** |

There is no document for the base image's packages. The release scans have
reported no finding in `gcr.io/distroless/static-debian13`; when one does, it
gets a hand-written `ferrofed-os.openvex.json` with one statement per finding,
naming both images. The images hold one static binary and no dynamic loader,
so an argument about a shared library has nothing to rest on, and the ELF
linkage check FerroEHR runs for its glibc images does not apply here.

## The generated document

`rust-advisories.openvex.json` is produced by
`scripts/security/vex-generate.sh` from two inputs, and is never edited by
hand:

- **`deny.toml`** `[advisories].ignore`: the authoritative set of advisory
  ids. It is the gate that decides whether a build passes, so it is the only
  place the id list lives.
- **`security/vex/rust-advisories.toml`**: the reasoning, the OpenVEX
  `status`, the `justification` and the `impact_statement` for each id.

The generator refuses to emit anything unless the two sets match in both
directions, and `scripts/checks/vex-advisories.sh`, the `vex` CI job,
regenerates the document and fails on any difference. An ignore added to
`deny.toml` without a published justification is a red build.

Agreement is not truth: an ignore and its justification stay in agreement
while a dependency upgrade resolves the advisory underneath both.
`scripts/checks/advisory-exceptions.sh`, in the `cargo-deny` CI job, promotes
cargo-deny's `advisory-not-detected` diagnostic to an error, so an exception
that has outlived its finding fails the build.

A statement can also agree, describe an advisory that still fires, and argue
from a dependency path that is not the real one. So each statement carries
its path as data (`carriers`, `workspace_roots`), and
`scripts/checks/vex-reachability.sh`, also in the `cargo-deny` job, compares
both sets with the graph cargo resolves, in both directions.

To change a statement: edit `rust-advisories.toml` (and `deny.toml` if the id
set changes), bump the document's `version` and `timestamp`, then run
`scripts/security/vex-generate.sh`. `carriers` and `workspace_roots` come from
`cargo tree -i <crate>[@<version>] --workspace --all-features --target all -e
normal,build,dev`.

## Where the scanners read them

- **The release gate** scans each image by digest before it is tagged, for
  HIGH and CRITICAL findings that have a fix (`trivy.yaml`). It passes no
  VEX: such a finding is fixed by a rebuild.
- **The release record** scans every image and binary with no floor, and
  `scripts/checks/vex-coverage.sh` joins the findings with these documents
  into `ferrofed-vX.Y.Z-vulnerability-record.json`, attached to the release.
- **The weekly scan** (`.github/workflows/image-scan.yml`) reads the
  published images with every document here passed as `--vex`, and
  `scripts/security/scan-images.sh` replays it locally.
