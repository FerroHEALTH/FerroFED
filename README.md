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
<!-- badges:end -->

An openEHR federation gateway, in pure Rust: where else the record is.

A record held by another organisation is out of reach today. FerroFED is a transparent ITS-REST intermediary: a client sends it an ordinary AQL query and never learns it was federated. The gateway resolves the patient first, through the index, so no directly identifying identifier travels in a query; then it sends standard AQL to each node, the local FerroEHR or a remote CDR, scoped to that node's own EHR id, and merges what comes back with each node's provenance. It holds no clinical data of its own. It follows the openEHR Federation Working Group's Federation Tier with AQL proposal.

FerroFED is one of the [FerroHEALTH](https://ferrohealth.eu/) family. The family
page shows where it sits among the eight and what calls what, and this
repository is where the design and the build happen; the tracker is the
record of both. The site is <https://ferrofed.eu/>, with the documentation
under [`/docs/`](https://ferrofed.eu/docs/).

## Quickstart

The gateway beside two member CDRs, FerroEHR and EHRbase, from the binaries of
the latest release:

```sh
scripts/release/stage-dist.sh 0.0.1
docker compose up --build --wait
curl http://127.0.0.1:8080/health
```

The gateway answers its health family today, and the federated query over the
two nodes lands with v0.0.2. The image, the ports and the development
credentials are described in
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
