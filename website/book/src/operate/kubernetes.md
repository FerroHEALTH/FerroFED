<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The Kubernetes package

FerroFED ships a kustomize package for Kubernetes, `deploy/kubernetes/`, and
every release attaches it with each image pinned by digest. It has three
parts:

| Part | What it holds |
|---|---|
| `base/` | the gateway: its StatefulSet, ConfigMap, Service, the headless Service of its admin listeners, its PodDisruptionBudget and its NetworkPolicy |
| `components/` | the parts you add: the operator console, an Ingress or a Gateway API route for each, a Prometheus Operator ServiceMonitor, and an egress NetworkPolicy |
| `overlays/example/` | an overlay over the base with every component, the shape a production install takes |

The base is the StatefulSet that [The container image](container.md#kubernetes)
describes, with the probes, the audit spool claim per replica and the
hardened security context. CI renders the base and the example overlay with
`kustomize` and validates every object with `kubeconform` in strict mode, and
runs `ferrofed config check` over the base's configuration and the
console's configuration check over the console component's, each with
synthetic secrets.

## Installing a release

Each release attaches `ferrofed-vX.Y.Z-kubernetes.tar.gz` and its SHA-256.
The tarball is the tree of that tag, with the gateway's image pinned to its
digest in `base/kustomization.yaml` and the console's in
`components/console/kustomization.yaml`, so the pods run exactly the image
the release scanned and attested:

```sh
gh release download vX.Y.Z --repo FerroHEALTH/FerroFED \
  --pattern 'ferrofed-vX.Y.Z-kubernetes.tar.gz*'
sha256sum -c ferrofed-vX.Y.Z-kubernetes.tar.gz.sha256sum
tar -xzf ferrofed-vX.Y.Z-kubernetes.tar.gz
```

Make an overlay of your own beside it, which names the base and the
components you use, and keep your edits there:

```yaml
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization
namespace: ferrofed
resources:
  - ../ferrofed-vX.Y.Z-kubernetes/base
components:
  - ../ferrofed-vX.Y.Z-kubernetes/components/console
  - ../ferrofed-vX.Y.Z-kubernetes/components/ingress
  - ../ferrofed-vX.Y.Z-kubernetes/components/console-ingress
  - ../ferrofed-vX.Y.Z-kubernetes/components/service-monitor
  - ../ferrofed-vX.Y.Z-kubernetes/components/egress
```

Every value to replace is marked `EDIT` in the manifest that holds it: the
issuer and audience of `[auth]`, the members of `registry.toml`, the storage
class of the audit spool, the namespaces of your reverse proxy and your
Prometheus, the hosts and certificates of the routes, and the egress
destinations. Create the Secrets the manifests name, then apply:

```sh
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-384 -out signing-key.pem
kubectl -n ferrofed create secret generic ferrofed-secrets \
  --from-file=signing-key.pem --from-file=metrics-scrape-hash
kubectl -n ferrofed create secret generic ferrofed-console-secrets \
  --from-file=client-secret=./console-client-secret
kubectl apply -k my-overlay/
```

The tree in the repository names the images by tag alone; only the release
tarball pins the digests. To pin them yourself, add an `images` entry with
the `digest` of the image you verified
([Verifying the image](container.md#kubernetes)).

## The components

| Component | What it adds |
|---|---|
| `console` | the operator console: a Deployment of one replica (its sessions live in its memory), its ConfigMap, its Service and a NetworkPolicy that admits the reverse proxy alone. It reaches the gateway's Service, and each replica's admin listener at `ferrofed-0.ferrofed-admin` and so on, for the configuration page |
| `ingress`, `console-ingress` | an Ingress for the gateway and for the console, over TLS |
| `gateway-api`, `console-route` | a Gateway API `HTTPRoute` for each, behind a Gateway whose HTTPS listener terminates TLS; use these or the Ingresses |
| `service-monitor` | a Prometheus Operator `ServiceMonitor` that scrapes each replica's admin listener through the headless Service, with the scrape user's Basic credentials from the `ferrofed-scrape` Secret ([Metrics](metrics.md)) |
| `egress` | an egress NetworkPolicy: DNS, and the destinations you list |

Behind either proxy, list the proxy's addresses in `server.trusted_proxies`
so the access record names the client's own address, and give the proxy a
body limit and a timeout at least `server.body_limit_bytes` and
`server.request_timeout_ms` ([A production deployment](production.md)).

## Network policies

The base's NetworkPolicy admits two sources to the gateway pods, and nothing
else:

- the gateway port: your reverse proxy's namespace, and the console pods;
- the admin port: your Prometheus pods, and the console pods.

A client inside the cluster that calls the gateway directly, past the proxy,
needs a peer of its own under the gateway port. The policies hold only where
the cluster's network plugin enforces NetworkPolicy, and they add to client
authentication, never replace it ([Hardening](hardening.md)).

Egress stays open in the base, because the gateway reaches its members, its
identity services and its issuers wherever they are. The `egress` component
closes it to DNS and the destinations you list, each one the configuration
names: every member endpoint of `registry.toml`, the issuers of `[auth]` and
`[[trust.issuer]]`, the Audit Record Repository, each identity service a
binding names, the token endpoint of each onward grant, the OTLP collector,
a stored-query or revision PostgreSQL, and the usage report's collector
unless the report is off. A NetworkPolicy matches addresses, so a
destination behind changing addresses needs a CIDR wide enough to hold
them. A destination you leave out is refused: a member shows `offline` and
a service `unavailable` in `GET {base}/operator/dependencies`.

## Spread, disruption and stopping

The StatefulSet spreads its replicas over zones and over nodes
(`topologySpreadConstraints` with `ScheduleAnyway`), so losing one zone or
one node leaves a replica serving, while a cluster with one zone or one node
still schedules both. The PodDisruptionBudget keeps one replica serving
through a node drain.

A pod's stop has two holds before the drain. The `preStop` hook sleeps for 5
seconds, matching `server.drain_delay_ms`, while the endpoint controllers and
the proxies stop sending the pod new requests; the gateway then turns
readiness to `503`, keeps accepting for the drain delay, and drains
([Stopping without dropping a request](health.md#stopping-without-dropping-a-request)).
`terminationGracePeriodSeconds: 50` outlasts the sleep, the delay, the drain
and the bindings' drain with 5 seconds of room, which
`scripts/checks/kubernetes-example.sh` holds in CI. Change the sleep with the
delay, and the grace period with either.

## Reloading a changed configuration

The ConfigMaps keep fixed names, so `kubectl apply -k` updates them in place
and no pod restarts. The kubelet then writes the new files into each pod's
mounted volume, within its sync period, which is about a minute by default
([ConfigMaps](https://kubernetes.io/docs/concepts/configuration/configmap/#mounted-configmaps-are-updated-automatically)).
The gateway never watches its files, so tell each replica to reload once the
files have changed. The image has no shell to send `SIGHUP` from, so ask each
replica's admin listener, with an access token that carries
`auth.product_scope`:

```sh
for pod in $(kubectl -n ferrofed get pods -l app.kubernetes.io/component=gateway -o name); do
  kubectl -n ferrofed port-forward "$pod" 9464:9464 &
  forward=$!
  sleep 2
  curl -fsS -X POST -H "Authorization: Bearer $OPERATOR_TOKEN" http://127.0.0.1:9464/admin/reload
  kill "$forward"
done
```

Each replica reads the configuration again, checks it whole and applies it
or refuses it, and its answer names what changed and what waits for a
restart ([Reloading the registry](registry.md#reloading-the-registry),
[Reloading over the admin listener](metrics.md#reloading-over-the-admin-listener)).
A key that needs a restart takes effect on the next rollout, `kubectl -n
ferrofed rollout restart statefulset/ferrofed`. Under `config.ui =
"managed"` the operator console saves, applies and confirms a revision, and
every replica follows it with no reload by hand
([Configuration revisions](configuration-revisions.md)).

The console reads its configuration at start alone: after a change to
`ferrofed-console-config`, restart it with `kubectl -n ferrofed rollout
restart deployment/ferrofed-console`.
