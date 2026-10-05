- Outside the development profile, a gateway with a registry needs an
  `[audit] destination`, because every access to patient data is now
  recorded (#623). `config check`, `serve` and a reload refuse the
  configuration without one, naming `audit.destination`. A build without
  the IHE binding records no access and refuses a registry outside
  development. The example Kubernetes ConfigMap sends the records to the
  log target. The `log` destination never names the client of a request
  the gateway received.
- `ihe-iti` 0.0.27 writes a BALP entity with `detail` entries, a user's
  organisation as an agent of its own and their alternative identity as
  `altId`, and a record that claims no pattern. `openehr-federation` 0.0.43
  reads the archetype and template ids a bound query constrains its data
  to, and whether they bind every class it reads (#623).
