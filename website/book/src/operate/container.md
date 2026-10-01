<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The container image and the quickstart

FerroFED ships one static binary, `ferrofed`, and an image that carries it on
distroless static. The repository's `compose.yaml` starts that image beside
two member CDRs of different products, FerroEHR and EHRbase, so the topology a
federated query runs over is up in one command.

## The image

`docker/Dockerfile` copies the release lane's musl binary for the target
platform onto `gcr.io/distroless/static-debian13:nonroot`, pinned by the
digest of its image index. The image:

- runs as the numeric user `65532:65532`, so an orchestrator's
  `runAsNonRoot` accepts it;
- has no shell and no package manager;
- needs no writable path, so it runs with a read-only root filesystem and every
  capability dropped;
- binds `0.0.0.0:8080` (the binary's own default is loopback, which no
  container can publish), set through `FERROFED__SERVER__LISTEN`;
- starts `ferrofed serve` as PID 1, so `SIGTERM` reaches the server and it
  drains before it exits.

The image declares no `HEALTHCHECK`. The base has no HTTP client and the
binary has no probe subcommand, so probe it from outside: `GET /health`
answers `200` while the process is up, and `GET /health/readiness` answers
`200` when every registered indicator is up.

The image lane publishes it as `ghcr.io/ferrohealth/ferrofed`. To build it
yourself from the binaries of a published release:

```sh
scripts/release/stage-dist.sh 0.0.1
docker buildx build -f docker/Dockerfile --platform linux/arm64 \
  -t ghcr.io/ferrohealth/ferrofed:0.0.1 --load .
```

The stage script checks every tarball against the `.sha256sum` published
beside it before it unpacks a byte.

## The quickstart

```sh
scripts/release/stage-dist.sh 0.0.1
docker compose up --build --wait
curl http://127.0.0.1:8080/health
```

| Service | What it is | On the host |
|---|---|---|
| `ferrofed` | the gateway | `127.0.0.1:8080` |
| `ferroehr`, `ferroehr-postgres` | member node A, FerroEHR on its own PostgreSQL image | `127.0.0.1:8081/ferroehr/rest/openehr/v1` |
| `ehrbase`, `ehrbase-db` | member node B, EHRbase on its companion PostgreSQL 16.2 image | `127.0.0.1:8091/ehrbase/rest/openehr/v1` |

Each node runs its product's documented image, database included, which is why
EHRbase keeps PostgreSQL 16.2: the PostgreSQL 18 rule covers FerroFED's own
database only. Both nodes use their products' quickstart Basic-auth user,
`ferroehr` / `ferroehr`, a development credential that must not reach anything
real. Every image is pinned by tag and digest, and `docs/VERSIONS.md` carries
each pin.

Every published port binds the loopback interface. A published port is
DNAT'd ahead of the host firewall's own rules, so a port on `0.0.0.0` is
reachable from the network even when the firewall says otherwise. Set
`FERROFED_BIND_HOST` to the one address you mean, or put a reverse proxy in
front.

Today the gateway serves its process shape: `/`, the health family, and `501`
under `/v1/`. The registry document that names the two nodes and the federated
query over them land with the v0.0.2 milestone, and the nodes above are the
ones that query reaches.

`docker compose down -v` stops the stack and removes its volumes.
