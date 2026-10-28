- If you edited the quickstart's `compose.yaml` or
  `docker/quickstart/ferropix/ferropix.toml`, carry FerroEHR 5.5.0's and
  FerroPIX 0.0.10's renames over: in `ferroehr-config`, `server.bind` is
  `server.listen`, each Basic user's roles are its own `roles` list in place
  of `[authz.rbac.basic_roles]`, and `[ehr_index] provider = "local"` is
  required; each node sets `FERROEHR_CONFIG`, `FERROEHR__REST__SYSTEM_ID`
  and `FERROEHR__TELEMETRY__FORMAT` in place of `FERROEHR__SERVER__SYSTEM_ID`
  and `FERROEHR__LOG__FORMAT`; FerroPIX's `server.public_base_url` is the
  top-level `public_base_url`. Run `docker compose down -v` once before the
  first start on the new images (#1386).
