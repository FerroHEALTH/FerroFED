<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->
# <img src="https://raw.githubusercontent.com/FerroHEALTH/FerroFED/main/assets/brand/ferrofed-icon.svg" alt="" width="40" height="40" align="top"> FerroFED

<!-- badges:begin -->
[![CI](https://github.com/FerroHEALTH/FerroFED/actions/workflows/ci.yml/badge.svg)](https://github.com/FerroHEALTH/FerroFED/actions/workflows/ci.yml)
[![CodeQL](https://github.com/FerroHEALTH/FerroFED/actions/workflows/codeql.yml/badge.svg)](https://github.com/FerroHEALTH/FerroFED/actions/workflows/codeql.yml)
[![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/FerroHEALTH/FerroFED/badge)](https://scorecard.dev/viewer/?uri=github.com/FerroHEALTH/FerroFED)
[![OpenSSF Best Practices](https://www.bestpractices.dev/projects/15130/badge)](https://www.bestpractices.dev/projects/15130)
[![Quality Gate Status](https://sonarcloud.io/api/project_badges/measure?project=rubentalstra_FerroFED&metric=alert_status)](https://sonarcloud.io/summary/overall?id=rubentalstra_FerroFED)
[![Coverage](https://sonarcloud.io/api/project_badges/measure?project=rubentalstra_FerroFED&metric=coverage)](https://sonarcloud.io/summary/new_code?id=rubentalstra_FerroFED)
[![License: BUSL-1.1](https://img.shields.io/badge/License-BUSL--1.1-blue.svg)](LICENSE)
[![GitHub release (latest SemVer)](https://img.shields.io/github/v/release/FerroHEALTH/FerroFED?sort=semver)](https://github.com/FerroHEALTH/FerroFED/releases/latest)
[![Image pulls](https://img.shields.io/badge/dynamic/json?url=https%3A%2F%2Fghcr-badge.elias.eu.org%2Fapi%2FFerroHEALTH%2FFerroFED%2Fferrofed&query=downloadCount&label=image%20pulls&logo=github)](https://github.com/FerroHEALTH/FerroFED/pkgs/container/ferrofed)
<!-- badges:end -->

<!-- conformance:begin -->
[![Federation Tier 0.9.0 gateway points](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroFED%2Fmain%2Fconformance%2Fbadges%2Ffederation-gateway.json)](https://ferrofed.eu/docs/evaluate/conformance.html)
[![Federation Tier 0.9.0 node points](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroFED%2Fmain%2Fconformance%2Fbadges%2Ffederation-node.json)](https://ferrofed.eu/docs/evaluate/conformance.html)
[![Federation Tier 0.9.0 operator points](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroFED%2Fmain%2Fconformance%2Fbadges%2Ffederation-operator.json)](https://ferrofed.eu/docs/evaluate/conformance.html)
[![AQL golden cases](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroFED%2Fmain%2Fconformance%2Fbadges%2Faql-golden.json)](conformance/aql-golden/pass-list.txt)
<!-- conformance:end -->

An openEHR federation gateway, in pure Rust: where else the record is.

A record held by another organisation is out of reach today. FerroFED is a transparent ITS-REST intermediary: a client sends it an ordinary AQL query and never learns it was federated. The gateway resolves the patient first, through the index, so no directly identifying identifier travels in a query; then it sends standard AQL to each node, the local FerroEHR or a remote CDR, scoped to that node's own EHR id, and merges what comes back with each node's provenance. It holds no clinical data of its own. It follows the openEHR Federation Working Group's Federation Tier with AQL proposal.

FerroFED is one of the [FerroHEALTH](https://ferrohealth.eu/) family. The family
page shows where it sits among the eight and what calls what. The design of
record is [`docs/architecture.md`](docs/architecture.md), and the tracker is
the record of the build: each milestone is a release, and the open issues are
the worklist. The site is <https://ferrofed.eu/>, with the documentation under
[`/docs/`](https://ferrofed.eu/docs/).

## Install

Every release on the
[releases page](https://github.com/FerroHEALTH/FerroFED/releases/latest) ships
the `ferrofed` binary for x86_64 and aarch64 Linux, on glibc and on musl, each
tarball with its checksum, SLSA provenance and SBOMs. The image
`ghcr.io/ferrohealth/ferrofed` carries the musl binary for `linux/amd64` and
`linux/arm64`, tagged with the release version and `latest`. Verify what you
download before you run it:

```sh
gh attestation verify ferrofed-vX.Y.Z-x86_64-unknown-linux-musl.tar.gz \
  --repo FerroHEALTH/FerroFED \
  --signer-workflow FerroHEALTH/FerroFED/.github/workflows/release-build.yml
gh attestation verify oci://ghcr.io/ferrohealth/ferrofed:X.Y.Z \
  --repo FerroHEALTH/FerroFED \
  --signer-workflow FerroHEALTH/FerroFED/.github/workflows/release-image.yml
```

`ferrofed serve --config ferrofed.toml` runs the gateway, and
`ferrofed config check --config ferrofed.toml` reports whether it would start
on that file. The
[configuration page](https://ferrofed.eu/docs/operate/configuration.html)
covers the registry, the identity service and every key.

## Quickstart

The gateway beside two member CDRs, two FerroEHR instances. `compose.yaml`
runs the published image of the current release:

```sh
docker compose up --wait
curl http://127.0.0.1:8080/health
```

Create one EHR on each node, then send one ordinary ITS-REST query to the
gateway:

```sh
curl -u ferroehr:ferroehr -X POST -H 'Prefer: return=minimal' \
  http://127.0.0.1:8081/ferroehr/rest/openehr/v1/ehr
curl -u ferroehr:ferroehr -X POST -H 'Prefer: return=minimal' \
  http://127.0.0.1:8082/ferroehr/rest/openehr/v1/ehr

curl http://127.0.0.1:8080/v1/query/aql \
  -H 'Content-Type: application/json' \
  -d '{"q":"SELECT e/ehr_id/value FROM EHR e"}'
```

The answer is one ITS-REST `RESULT_SET` with the EHR of each node in `rows`,
and `meta.federation` reports both endpoints `active`. The quickstart
configuration (`docker/quickstart/`) names the two nodes and binds no identity
service, so a query that names a patient fails closed with `424`. The image,
the ports, the development credentials and building the image from the
release binaries are described in
[the container page](https://ferrofed.eu/docs/operate/container.html).

## Licence

FerroFED is source-available under the Business Source License 1.1. The
parameters that apply, the Licensor, the Licensed Work, the Additional Use
Grant and the Change Date, are in [LICENSE](LICENSE): free for non-commercial
production use, a commercial licence for any other production use, and Apache
2.0 four years after each version is published. The maintainer named in
[MAINTAINERS.md](MAINTAINERS.md) is the contact for a commercial licence.

The brand assets under `assets/brand/` are part of the Licensed Work.

Contributions carry the terms in
[CONTRIBUTING.md](CONTRIBUTING.md#licensing-of-contributions): you keep your
copyright, and you grant the Licensor the relicensing right that keeps the work
one work under one licensor. There is no separate agreement to sign.
