- The compose file every release carries runs the operator console beside
  the gateway under a `console` profile (`docker compose --profile console
  up`), from `ghcr.io/ferrohealth/ferrofed-console` at the release's
  version, with the gateway's hardening, bound to 127.0.0.1, its OIDC
  client secret read as a file secret (#1258).
