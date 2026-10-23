- Breaking: the operator console is renamed from `ferrofed-viewer` to
  `ferrofed-console` (#1219): the crate and its binary
  (`app/ferrofed-console`), the image (`ghcr.io/ferrohealth/ferrofed-console`
  in place of `ghcr.io/ferrohealth/ferrofed-viewer`, which is not published
  again), the release workflow and its attestation signer
  (`.github/workflows/release-console.yml`), the Dockerfile
  (`docker/console/Dockerfile`), the site and bundle scripts
  (`scripts/release/console-site.sh`, `console-tools.sh`,
  `scripts/checks/console-bundle.sh`, `console-paths.sh`), the CI job
  (`console`), the configuration file variable (`FERROFED_CONSOLE_CONFIG`),
  the environment prefix (`FERROFED_CONSOLE__`), the cookies
  (`ferrofed_console_session`, `ferrofed_console_sign_in`,
  `ferrofed_console_theme`, each with the `__Host-` prefix where secure) and
  the catalog (`locales/en/ferrofed-console.ftl`).
