- The book has a production guide that walks a deployment from nothing to a
  first federated query over two CDRs (#624): what to have in place, a
  checklist each CDR meets before admission, verifying and installing the
  release, the registry, an issuer recipe for Keycloak 26.2 or later that
  issues the RFC 9068 tokens the gateway accepts with the SMART on openEHR
  scopes and the purpose of use, the onward credentials, the PIX Manager,
  the access log, TLS, the checks before the start, admission, the first
  query and the operations that follow. The Keycloak recipe was verified
  against Keycloak 26.8.0.
- `deploy/nginx/ferrofed.conf` is a reverse proxy for the gateway: TLS on the
  public address, the base path, the JWK Set and PMIR feed routes passed, the
  health family kept inside, and an access log that names the gateway's
  request id and never a query string (#624).
  `scripts/checks/production-guide.sh` runs `config check` over every TOML
  block of the guide, then serves that configuration behind the shipped
  file in nginx and checks each route; CI runs it in the release compose job.
