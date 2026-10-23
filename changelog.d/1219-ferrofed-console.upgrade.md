- The operator console is now `ferrofed-console` (#1219). Pull
  `ghcr.io/ferrohealth/ferrofed-console` in place of
  `ghcr.io/ferrohealth/ferrofed-viewer`, rename every
  `FERROFED_VIEWER__<SECTION>__<KEY>` override to
  `FERROFED_CONSOLE__<SECTION>__<KEY>` and `FERROFED_VIEWER_CONFIG` to
  `FERROFED_CONSOLE_CONFIG`, and run the binary as `ferrofed-console`. The
  session and theme cookies are renamed, so every operator signs in once
  more after the upgrade.
