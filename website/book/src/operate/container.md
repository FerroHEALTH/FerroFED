<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The container image and the quickstart

FerroFED ships one static binary, `ferrofed`, and an image that carries it on
distroless static. Every release carries `compose.yaml`, which runs that image
alone in front of the CDRs you already run, with an example `ferrofed.toml`
and `registry.toml` ([The gateway from a
release](#the-gateway-from-a-release)). The repository's own `compose.yaml`
starts the image beside four member CDRs, four FerroEHR instances, so the
topology a federated query runs over is up in one command ([The
quickstart](#the-quickstart)).

See how it works: [the quickstart and a production layout](../how-it-works/deployment.md).

## The gateway from a release

Every release carries three files that run the gateway alone, at that
release's image, in front of the CDRs you already run, and the nginx
reverse proxy that puts it on its public address. You need Docker Compose
and no checkout of the repository.

1. Download the files into a directory of their own, and check the proxy
   configuration against its checksum:

   ```sh
   mkdir ferrofed && cd ferrofed
   for f in compose.yaml ferrofed.toml registry.toml ferrofed.conf ferrofed.conf.sha256sum; do
     curl -LO "https://github.com/FerroHEALTH/FerroFED/releases/latest/download/$f"
   done
   sha256sum -c ferrofed.conf.sha256sum
   ```

   `…/releases/download/vX.Y.Z/$f` downloads the files of one version.
   `ferrofed.conf` is the nginx configuration of the
   [production guide](production.md#8-tls-and-the-public-address); Compose
   does not read it.
2. Edit `registry.toml`, the [registry document](registry.md): replace the two
   example members with your organisations, nodes and endpoints, as many as the
   federation has.
3. Edit `ferrofed.toml`, the [gateway configuration](configuration.md): your
   federation id, your PIX Manager's URL with each node's `ehr_id` domain there
   ([Identity resolution](identity.md)), and a `[credentials."<endpoint id>"]`
   section for each endpoint that needs one, and `[signing] jwks_uri`, the
   address the nodes fetch the gateway's JWK Set from. Every value to change
   is marked `EDIT`.
4. Put each credential, and the gateway's signing key, in its own file in
   `secrets/`, under the name `ferrofed.toml` gives it after
   `/run/secrets/ferrofed/`. The gateway runs as uid 65532, so that user must
   be able to read each file:

   ```sh
   mkdir -p secrets
   printf '%s\n' "$PIX_TOKEN" > secrets/pix-token
   openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-384 \
     -out secrets/signing-key.pem
   sudo chown -R 65532:65532 secrets && sudo chmod 0400 secrets/*
   ```

5. Start it:

   ```sh
   docker compose up --wait
   curl http://127.0.0.1:8080/health
   ```

This setup writes no credential in `ferrofed.toml`: each one is a file, named
by a `bearer_token_file` or `password_file` key, and so is the signing key,
named by `key_file`. The gateway also accepts a secret written inline, under
every profile ([Configuration](configuration.md#secrets-inline-or-from-a-file)),
but a file keeps it out of the configuration and out of every copy of it. Compose mounts `ferrofed.toml`, `registry.toml` and
`secrets/` read-only at `/etc/ferrofed/` and `/run/secrets/ferrofed/`, and
the named volume `audit-spool` at `/var/lib/ferrofed` for the
[audit spool](#the-audit-spool). A missing `ferrofed.toml` or `registry.toml`
stops `docker compose up`; a missing `secrets/` is created empty.

A configuration the gateway refuses stops it with exit code 78, and
`docker compose up --wait` reports the container unhealthy. `docker compose
logs ferrofed` shows the one line naming the key at fault. After you correct a
file, `docker compose restart ferrofed` starts the gateway on it.

The variables in the compose file are about the container only, read from the
shell or from `.env` beside it:

| Variable | Default | What it sets |
|---|---|---|
| `FERROFED_VERSION` | the release's version | the image tag |
| `FERROFED_BIND_HOST` | `127.0.0.1` | the host address the gateway is published on |
| `FERROFED_PORT` | `8080` | the host port |
| `FERROFED_CPUS` | `1` | the CPU limit |
| `FERROFED_MEMORY` | `256M` | the memory limit |

The gateway service runs as uid 65532 with a read-only root filesystem, every
capability dropped and `no-new-privileges`. Its healthcheck is the image's
`ferrofed healthcheck`, so `--wait` returns once the gateway is ready. Its
port binds the loopback interface unless you set `FERROFED_BIND_HOST`, for the
reason under [The quickstart](#the-quickstart). A registry URL can name a CDR
on the Docker host itself as `host.docker.internal`. The stop grace period of
40 seconds outlasts the gateway's 30-second drain and the bindings' 5-second
drain
([Stopping without dropping a request](health.md#stopping-without-dropping-a-request));
raise it with `server.drain_delay_ms`, `server.shutdown_timeout_ms` or
`server.bindings_drain_timeout_ms`.

The restart policy is `unless-stopped`. Docker restarts a gateway that exits
with an error, doubling its wait before each attempt from 100 ms
([`docker run --restart`](https://docs.docker.com/reference/cli/docker/container/run/)),
so a refused configuration is retried at a slowing pace rather than in a
tight loop. `on-failure` would stop the retries after a count, but Docker
does not apply it when the daemon restarts
([restart policies](https://docs.docker.com/engine/containers/start-containers-automatically/)),
so a gateway that drained cleanly on a host reboot would stay down. Docker restarts no container
for failing its healthcheck; the healthcheck tells `--wait` and you whether the
gateway is ready.

To move to a newer release, download its `compose.yaml` over the old one and
run `docker compose up --wait`. Read the release's Upgrade notes first, and
follow [Upgrading](upgrading.md); [Rollback](rollback.md) says how to go back.

The files carry no development cross-reference. That table is for trials only,
and the quickstart below carries it; a deployment resolves patients through
its PIX Manager.

## The image

`docker/Dockerfile` copies the release lane's musl binary for the target
platform onto `gcr.io/distroless/static-debian13:nonroot`, pinned by the
digest of its image index. The image:

- runs as the numeric user `65532:65532`, so an orchestrator's
  `runAsNonRoot` accepts it;
- has no shell and no package manager;
- runs with a read-only root filesystem and every capability dropped, and
  writes only to the [audit spool](#the-audit-spool) volume at
  `/var/lib/ferrofed`;
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

`ferrofed healthcheck` asks the gateway's readiness over loopback and exits
`0` only when it answers `200`; [Health probes](health.md) has the details.
`docker inspect --format '{{.State.Health.Status}}' <container>` shows the
outcome.

### The audit spool

With `[xcpd] audit = "repository"`, the gateway keeps each ITI-55 audit
message in a spool until the ATNA Audit Record Repository takes it
([The audit repository](localization.md#the-audit-repository)). Set
`xcpd.audit_repository.spool_dir = "/var/lib/ferrofed/audit-spool"`. The
image ships `/var/lib/ferrofed` and that spool directory owned by the
gateway's user `65532:65532` with mode `0700`, so a named Docker volume
mounted at `/var/lib/ferrofed` starts with that owner and mode, and the
release `compose.yaml` mounts one, `audit-spool`, with no step on the host.

With `[audit] destination = "repository"`, the PIXm, mCSD and PMIR audit
records wait in a spool of their own until the repository takes them
([The audit trail](audit.md)). Set
`audit.repository.spool_dir = "/var/lib/ferrofed/audit-feed-spool"` on the
same volume.

The spool holds audit records that name patients: each message carries the
query parameters, the patient identifier among them. Keep the volume on an
encrypted disk; the gateway holds no key to encrypt it with. Each replica
needs a spool of its own, since two gateways must never drain one
directory.

#### Losing a spool

A record counts as recorded once it is on the spool's disk, and the
transaction it records goes on from there
([The spool and the failure policy](audit.md#the-spool-and-the-failure-policy)).
A spool that is lost before the repository took everything in it therefore
loses records of transactions that did happen: the repository's trail misses
them for good, and nothing at the repository shows the gap. The gateway
cannot send them again, because it keeps no other copy. While records wait,
the dependency report shows `audit_repository` or `audit_feed` as
`degraded` ([Health probes](health.md)), so a spool is safe to discard only
once both read `up`.

So keep the spool on storage that outlives the container and its host: a
named volume under Docker, and a persistent volume claim per replica under
Kubernetes. A replica that comes back on the same volume delivers what its
spool still holds.

## Kubernetes

`deploy/kubernetes/` holds an example: a ConfigMap with the configuration and
the registry document and no secret, a StatefulSet, a Service, a
PodDisruptionBudget and a NetworkPolicy that opens the metrics port to your
Prometheus alone ([Metrics](metrics.md#dashboard-and-alert-rules)). CI validates every manifest with `kubeconform` in strict
mode, and runs `ferrofed config check` over the ConfigMap's configuration
with synthetic secrets. The configuration trusts one example issuer in
`[auth]` and reads the gateway's signing key from the `ferrofed-secrets`
Secret, which you create before you apply the manifests.

The gateway runs as a StatefulSet for its audit spool alone. In a
Deployment, every pod mounts the same volume claim, or an `emptyDir` of its
own, and neither fits: two gateways must never drain one spool, and an
`emptyDir` is deleted with its pod, so a reschedule or a node drain would
[lose the records](#losing-a-spool) still in it. A StatefulSet's
`volumeClaimTemplates` gives each replica a claim of its own,
`audit-spool-ferrofed-0` and `audit-spool-ferrofed-1`, which the replica
finds again wherever it is scheduled
([StatefulSets](https://kubernetes.io/docs/concepts/workloads/controllers/statefulset/)).
The replicas share nothing else, so `podManagementPolicy: Parallel` starts
and stops them together, and the StatefulSet names no governing Service,
because no client addresses one replica. The StatefulSet:

- probes startup and readiness on `GET /health/readiness` and liveness on
  `GET /health`;
- mounts the ConfigMap at `/etc/ferrofed` and the `ferrofed-secrets` Secret
  at `/run/secrets/ferrofed`, readable by the gateway's group (`fsGroup`
  `65532`, mode `0440`);
- mounts its `audit-spool` claim at `/var/lib/ferrofed` for the
  [audit spool](#the-audit-spool), writable through the same `fsGroup`, which
  the kubelet applies only when the volume's root does not already match
  (`fsGroupChangePolicy: OnRootMismatch`); the gateway creates each spool
  under it with mode `0700`. The claim asks for the storage class
  `encrypted`, a placeholder: replace it with a class of your cluster that
  encrypts the volume at rest, because the spool names patients. Its `1Gi`
  holds the default bounds of both spools (`spool_max_bytes`,
  `spool_max_events`) with headroom; raise it with either bound;
- keeps each claim when the StatefulSet is deleted or scaled down
  (`persistentVolumeClaimRetentionPolicy` `Retain`). A replica scaled away
  leaves its claim behind, and the records in it wait until that replica
  comes back. Before you delete a claim, scale back up until its spool is
  delivered, and check that `GET {base}/operator/dependencies` reads `up` for the
  audit repository;
- runs as the numeric user `65532` with `runAsNonRoot`, a read-only root
  filesystem, `allowPrivilegeEscalation: false`, every capability dropped and
  the `RuntimeDefault` seccomp profile;
- sets resource requests and limits, a starting point to size from your own
  load, ephemeral storage among them (a `256Mi` limit for the container
  log, since the spool is on its claim);
- sets `server.drain_delay_ms` to 5 seconds in the example configuration, so
  a pod keeps accepting while it leaves the Service's endpoints, and
  `server.shutdown_timeout_ms` to the 30-second request timeout and
  `server.bindings_drain_timeout_ms` to 5 seconds;
- gives the pod a `terminationGracePeriodSeconds` of 45, which outlasts the
  delay, the drain and the bindings' drain with 5 seconds of room, so the
  kubelet never kills a drain in progress. Keep the grace period above the
  three if you change any of them
  ([Stopping without dropping a request](health.md#stopping-without-dropping-a-request)).

The PodDisruptionBudget keeps one of the two replicas serving through a
voluntary disruption. Every secret is a key of the `ferrofed-secrets`
Secret, named by a `_file` key ([Configuration](configuration.md)): the
signing key, and any credential an endpoint needs.

```sh
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-384 -out signing-key.pem
kubectl create secret generic ferrofed-secrets --from-file=signing-key.pem
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

The release lane is the only place the image is built: every compose file
and manifest in the repository runs the published image, and none builds
one.

## The release binaries

Every release on the
[releases page](https://github.com/FerroHEALTH/FerroFED/releases/latest)
carries `ferrofed` for `x86_64` and `aarch64` Linux, on glibc and on musl.
FerroFED runs on Unix only
([Supported platforms](deployment-shape.md#supported-platforms)), and no
release carries a Windows binary. Each tarball holds the binary, `LICENSE`, `NOTICE` and the README, and comes with
its `.sha256sum`, a CycloneDX and an SPDX SBOM, and the Sigstore bundles of its
provenance and SBOM attestations. Verify a tarball before you unpack it:

```sh
gh attestation verify ferrofed-vX.Y.Z-x86_64-unknown-linux-musl.tar.gz \
  --repo FerroHEALTH/FerroFED \
  --signer-workflow FerroHEALTH/FerroFED/.github/workflows/release-build.yml
```

## The quickstart

```sh
scripts/quickstart/signing-key.sh
docker compose up --wait
scripts/quickstart/seed.sh
curl http://127.0.0.1:8080/health
```

`scripts/quickstart/signing-key.sh` writes the gateway's development
signing key into `docker/quickstart/signing/`, which is never committed, and
keeps a key that exists. The gateway signs the caller's identity onto every
request to a node with it, and refuses to start without it
([What a node is told about the caller](authentication.md#what-a-node-is-told-about-the-caller)).

`--wait` returns once every service reports healthy. The gateway service
runs `ghcr.io/ferrohealth/ferrofed` at the product version, the tag default
`compose.yaml` holds, and `FERROFED_VERSION` selects another published
version. Its healthcheck is the image's own `ferrofed healthcheck`, which
images before v0.0.7 do not carry, so with an older `FERROFED_VERSION` the
gateway never turns healthy and `--wait` fails. No compose file builds the
image; every one runs the published image.

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
nodes in one database would share their tables. Each database admits only its
own node's role, because the script revokes `CONNECT` from `PUBLIC` and grants
it to that role, so node A's role is refused on `ferroehr_b`. The nodes still
share one server, its superuser and its cluster-wide group roles, so this is
no production boundary.

Four instances of one product: EHRbase, the second product the topology first
used, refuses a `.` in `PARTY_REF.namespace`, which openEHR BASE admits, so its
EHRs could not carry the OID-style issuing namespace the synthetic patients
use. Every node uses FerroEHR's quickstart Basic-auth user, `ferroehr` /
`ferroehr`, and every database role's password is its name followed by
`_example` (`ferroehr_a_example`): development credentials that must not
reach anything real. Every image is pinned by tag
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
a PIX Manager ([Identity resolution](identity.md)).

`docker compose down -v` stops the stack and removes its volumes. A
quickstart volume created before v0.0.8 holds the roles with their earlier
passwords, which the nodes no longer send, so run `docker compose down -v`
once before you bring that stack up again, then seed it anew.

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

Ask for the compositions of the patient every node knows. Every request
carries an access token: `scripts/quickstart/token.sh` generates a
development issuer's key pair with `openssl` on its first run, writes its key
set where the gateway reads it, and prints a token valid for an hour
([The quickstart issuer](authentication.md#the-quickstart-issuer)):

```sh
curl -s http://127.0.0.1:8080/v1/query/aql \
  -H "Authorization: Bearer $(scripts/quickstart/token.sh)" \
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
  -H "Authorization: Bearer $(scripts/quickstart/token.sh)" \
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
