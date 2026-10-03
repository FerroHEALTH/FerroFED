<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The container image and the quickstart

FerroFED ships one static binary, `ferrofed`, and an image that carries it on
distroless static. The repository's `compose.yaml` starts that image beside
four member CDRs, four FerroEHR instances, so the topology a federated query
runs over is up in one command.

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
scripts/quickstart/seed.sh
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
| `ferroehr-a` | member node A, FerroEHR with `system_id` `node-a.quickstart.local` | `127.0.0.1:8081/ferroehr/rest/openehr/v1` |
| `ferroehr-b` | member node B, FerroEHR with `system_id` `node-b.quickstart.local` | `127.0.0.1:8082/ferroehr/rest/openehr/v1` |
| `ferroehr-c` | member node C, FerroEHR with `system_id` `node-c.quickstart.local` | `127.0.0.1:8083/ferroehr/rest/openehr/v1` |
| `ferroehr-d` | member node D, FerroEHR with `system_id` `node-d.quickstart.local` | `127.0.0.1:8084/ferroehr/rest/openehr/v1` |
| `ferroehr-postgres` | one FerroEHR PostgreSQL server, a database per node | not published |

The four nodes run FerroEHR's documented image, and each stamps its own
`system_id` into every EHR and version it creates, the value the quickstart
registry declares for it. They share one FerroEHR PostgreSQL container, and
each connects to its own database there, `ferroehr_a` to `ferroehr_d`, owned
by a login role of the same name. The image's own init script creates the
first; `docker/postgres/20-ferrofed-node-databases.sh`, mounted beside it, runs
that script again for the other three. A schema per node would not do:
FerroEHR creates fixed schema names in the database it connects to, so two
nodes in one database would share their tables. The nodes also share the
server's group roles, so the separation is the quickstart's and no security
boundary.

Four instances of one product: EHRbase, the second product the topology first
used, refuses a `.` in `PARTY_REF.namespace`, which openEHR BASE admits, so its
EHRs could not carry the OID-style issuing namespace the synthetic patients
use. Every node uses FerroEHR's quickstart Basic-auth user, `ferroehr` /
`ferroehr`, and every database role's password is its name: development
credentials that must not reach anything real. Every image is pinned by tag
and digest, and `docs/VERSIONS.md` carries each pin.

Every published port binds the loopback interface. A published port is
DNAT'd ahead of the host firewall's own rules, so a port on `0.0.0.0` is
reachable from the network even when the firewall says otherwise. Set
`FERROFED_BIND_HOST` to the one address you mean, or put a reverse proxy in
front.

`docker/quickstart/registry.toml` names the four nodes and
`docker/quickstart/ferrofed.toml` configures the gateway with each node's
quickstart credentials; Compose mounts both read-only. The configuration runs
in the development profile, with a static cross-reference from four synthetic
patients to their EHRs. It is a testing device and no identity binding; a
deployment resolves patients through an identifier cross-reference service,
such as a PIXm Manager ([What FerroFED runs beside](deployment-shape.md)).

`docker compose down -v` stops the stack and removes its volumes.

### The synthetic patients

`scripts/quickstart/seed.sh` reads the cross-reference rows of
`docker/quickstart/ferrofed.toml` and creates exactly those EHRs over each
node's ITS-REST API: `PUT /v1/ehr/{ehr_id}` with the patient on
`EHR_STATUS.subject`, the vendored `International Patient Summary` template,
and one vendored demo composition per EHR. It needs `curl`, and a second run
reports what is already there and adds nothing. Every identifier lies in the
example arc `urn:oid:2.999`:

| Patient, in `urn:oid:2.999.1.1` | Node A | Node B | Node C | Node D |
|---|---|---|---|---|
| `ffd-test-0001` | `aaaaaaaa-…-000000000001` | `bbbbbbbb-…-000000000001` | `cccccccc-…-000000000001` | `dddddddd-…-000000000001` |
| `ffd-test-0002` | | `bbbbbbbb-…-000000000002` | | `dddddddd-…-000000000002` |
| `ffd-test-0003` | | | `cccccccc-…-000000000003` | |
| `ffd-test-0004` | | | | |

The first group of each `ehr_id` names its node and the last group its
patient.

### A federated query

Ask for the compositions of the patient every node knows:

```sh
curl -s http://127.0.0.1:8080/v1/query/aql \
  -H 'Content-Type: application/json' -d @- <<'EOF'
{"q": "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = 'ffd-test-0001' AND e/ehr_status/subject/external_ref/namespace = 'urn:oid:2.999.1.1'"}
EOF
```

The gateway resolves the patient to the four `ehr_id`s, sends each node an
ordinary AQL query scoped to its own `ehr_id` with no patient identifier in
it, and answers `200` with one `RESULT_SET`: four rows, one composition from
each node, its `uid` carrying the `system_id` of the node that created it.
`meta.federation.complete` is `true`, and `meta.federation.endpoints` reports
all four endpoints `active` with `row_count` 1.

### A patient missing at some nodes

Send the same query for `ffd-test-0002`. The answer is `200` with two rows,
from node B and node D. Node A and node C hold no EHR for the patient, so the
gateway asks neither of them and reports both `not-resolved`, with no
`latency_ms`, and `meta.federation.complete` is `false`. A `not-resolved`
member is an answer and never fails the query (§11.3, N6, N37). For
`ffd-test-0003` only node C answers, and for `ffd-test-0004`, which no node
holds, the answer is `200` with no rows and all four endpoints
`not-resolved`.

### A query directed at one node

Name the node in the query with the endpoint directive, and project its
attributes beside the data (§8, §9.4):

```sh
curl -s http://127.0.0.1:8080/v1/query/aql \
  -H 'Content-Type: application/json' -d @- <<'EOF'
{"q": "SELECT p/id AS endpoint_id, p/system_id AS system_id, c/uid/value AS composition FROM ENDPOINT p [\"node-c-query\"] CONTAINS EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = 'ffd-test-0001' AND e/ehr_status/subject/external_ref/namespace = 'urn:oid:2.999.1.1'"}
EOF
```

Only node C is asked. The one row reads `node-c-query`,
`node-c.quickstart.local` and the composition's `uid`; the other three
endpoints are reported `excluded`, "not named by the request's endpoint
directive", and `meta.federation.complete` is `true`, because an excluded
member was never in scope. The header form selects the same way without
changing the query: send the patient query above with
`-H 'openEHR-federation-endpoint: node-b-query'` and only node B answers.

### Memory footprint

Measured on 2026-10-03 with `docker stats --no-stream`, on an Apple silicon
Mac running Docker Desktop with 8 CPUs and 7.65 GiB for its virtual machine,
the images already pulled:

| When | Gateway | PostgreSQL | Each FerroEHR node | Total |
|---|---|---|---|---|
| right after `docker compose up --wait` (22 s) | 1.8 MiB | 145 MiB | 23 MiB | 238 MiB |
| after the seed and the queries on this page | 2.2 MiB | 185 MiB | 54 to 65 MiB | 425 MiB |
