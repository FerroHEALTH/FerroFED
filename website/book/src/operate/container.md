<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The container image and the quickstart

FerroFED ships one static binary, `ferrofed`, and an image that carries it on
distroless static. The repository's `compose.yaml` starts that image beside
two member CDRs, two FerroEHR instances, so the topology a federated query runs
over is up in one command.

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

The base has no shell and no HTTP client, so the image's `HEALTHCHECK` runs
the binary itself:

```dockerfile
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD ["/usr/local/bin/ferrofed", "healthcheck"]
```

`ferrofed healthcheck` reads the configuration the way `serve` does, asks
`GET /health/readiness` on the configured listen port over loopback, and
prints one line. It exits `0` only when readiness answers `200`, and `1` for
any other status, a refused connection, no answer within three seconds, or a
configuration that does not load. `docker inspect --format
'{{.State.Health.Status}}' <container>` shows the outcome.

## The health probes

| Route | Answers | Use it as |
|---|---|---|
| `GET /health` | `200` while the process serves; it checks nothing else | liveness |
| `GET /health/readiness` | `200` while the gateway serves and its own subsystems are up; `503` before boot completes and from the moment `SIGTERM` or `SIGINT` arrives | readiness, startup, the image `HEALTHCHECK` |
| `GET /health/dependencies` | always `200`, with the state the gateway last observed of each member endpoint and of the resolver | monitoring, never a probe |

Readiness reports the gateway's own subsystems by name: the configuration,
the registry and the outbound clients when a registry is configured, and the
stored-query store when one is. Its body names the phase of the process,
`booting`, `serving` or `draining`. On `SIGTERM` readiness turns `503` before
the drain starts, so a load balancer stops sending requests while the
requests in flight finish.

No member node and no identity source gates readiness. A node outage is
reported per query in `meta.federation` (§11), and a gateway that went unready
with one node would turn one CDR outage into a total outage. Their state is on
`GET /health/dependencies` instead:

```json
{
  "endpoints": { "node-a-query": "up", "node-b-query": "down" },
  "resolver": "up"
}
```

Each state is the one the last request the gateway made for a client
observed: `up` (it answered), `failing` (it answered with a failure), `down`
(unreachable, or no answer in time), or `unknown` (no request has reached it
since the registry was loaded or reloaded). The gateway sends no request of its
own to find out. `resolver` is absent when no resolver is configured. The body
names endpoint ids and states only, never a URL, a credential or a body.

## Kubernetes

`deploy/kubernetes/` holds an example: a ConfigMap with the configuration and
the registry document and no secret, a Deployment, a Service and a
PodDisruptionBudget. CI validates every manifest with `kubeconform` in strict
mode. The Deployment:

- probes startup and readiness on `GET /health/readiness` and liveness on
  `GET /health`;
- runs as the numeric user `65532` with `runAsNonRoot`, a read-only root
  filesystem, `allowPrivilegeEscalation: false`, every capability dropped and
  the `RuntimeDefault` seccomp profile;
- sets resource requests and limits, a starting point to size from your own
  load;
- gives the pod a `terminationGracePeriodSeconds` of 30, longer than the
  10-second `server.shutdown_timeout_ms` of the example configuration, so the
  kubelet never kills a drain in progress. Keep the grace period longer than
  the drain if you change either.

The PodDisruptionBudget keeps one of the two replicas serving through a
voluntary disruption. Credentials belong in a Secret mounted beside the
ConfigMap and named by a `_file` key ([Configuration](configuration.md)).

```sh
kubectl apply -f deploy/kubernetes/
```

The image lane publishes it as `ghcr.io/ferrohealth/ferrofed`, tagged with the
release version, its `major.minor` and `latest`, and attests the index and
each platform manifest. Verify the image you pull:

```sh
gh attestation verify oci://ghcr.io/ferrohealth/ferrofed:X.Y.Z \
  --repo FerroHEALTH/FerroFED \
  --signer-workflow FerroHEALTH/FerroFED/.github/workflows/release-image.yml
```

To build it yourself from the binaries of a published release, name that
release's version:

```sh
scripts/release/stage-dist.sh X.Y.Z
docker buildx build -f docker/Dockerfile --platform linux/arm64 \
  -t ghcr.io/ferrohealth/ferrofed:X.Y.Z --load .
```

The stage script checks every tarball against the `.sha256sum` published
beside it before it unpacks a byte.

## The release binaries

Every release on the
[releases page](https://github.com/FerroHEALTH/FerroFED/releases/latest)
carries `ferrofed` for `x86_64` and `aarch64` Linux, on glibc and on musl. Each
tarball holds the binary, `LICENSE`, `NOTICE` and the README, and comes with
its `.sha256sum`, a CycloneDX and an SPDX SBOM, and the Sigstore bundles of its
provenance and SBOM attestations. Verify a tarball before you unpack it:

```sh
gh attestation verify ferrofed-vX.Y.Z-x86_64-unknown-linux-musl.tar.gz \
  --repo FerroHEALTH/FerroFED \
  --signer-workflow FerroHEALTH/FerroFED/.github/workflows/release-build.yml
```

## The quickstart

```sh
docker compose up --wait
curl http://127.0.0.1:8080/health
```

`--wait` returns once every service reports healthy; the gateway's
healthcheck is the image's own `ferrofed healthcheck`, which an image of a
release before v0.0.7 does not have. The gateway service runs `ghcr.io/ferrohealth/ferrofed` at the current release,
the tag default `compose.yaml` holds equal to the product version, and
`FERROFED_VERSION` selects another published version. To run an image you
built from staged binaries instead, add `--build`.

| Service | What it is | On the host |
|---|---|---|
| `ferrofed` | the gateway | `127.0.0.1:8080` |
| `ferroehr-a`, `ferroehr-a-postgres` | member node A, FerroEHR with `system_id` `node-a.quickstart.local` | `127.0.0.1:8081/ferroehr/rest/openehr/v1` |
| `ferroehr-b`, `ferroehr-b-postgres` | member node B, FerroEHR with `system_id` `node-b.quickstart.local` | `127.0.0.1:8082/ferroehr/rest/openehr/v1` |

Both nodes run FerroEHR's documented image, each on its own FerroEHR PostgreSQL
container, and each stamps its own `system_id` into every EHR and version it
creates, the value the quickstart registry declares for it. Two instances of
one product: EHRbase, the second product the topology first used, refuses a
`.` in `PARTY_REF.namespace`, which openEHR BASE admits, so its EHRs could not
carry the OID-style issuing namespace the synthetic patients use. Both nodes
use FerroEHR's quickstart Basic-auth user, `ferroehr` / `ferroehr`, a
development credential that must not reach anything real. Every image is
pinned by tag and digest, and `docs/VERSIONS.md` carries each pin.

Every published port binds the loopback interface. A published port is
DNAT'd ahead of the host firewall's own rules, so a port on `0.0.0.0` is
reachable from the network even when the firewall says otherwise. Set
`FERROFED_BIND_HOST` to the one address you mean, or put a reverse proxy in
front.

The gateway federates the two nodes. `docker/quickstart/registry.toml` names the nodes and
`docker/quickstart/ferrofed.toml` configures the gateway with each node's
quickstart credentials; Compose mounts both read-only. `POST /v1/query/aql`
answers one ITS-REST `RESULT_SET` over both nodes, with `meta.federation`
reporting each endpoint, and every other path under `/v1/` answers `501`. The
quickstart binds no identity service, so a query that names a patient fails
closed with `424`; the README's quickstart sends one that names none.

`docker compose down -v` stops the stack and removes its volumes.
