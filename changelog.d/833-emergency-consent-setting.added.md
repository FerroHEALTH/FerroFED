- `[federation.consent] emergency` chooses what the consent pre-filter does
  for a request whose verified token declares an
  `[[access_log.emergency_purpose]]` (#833). Under `"apply"`, the default,
  the pre-filter applies as to any request. Under `"pass-to-node"`, the
  pre-filter is still asked, and every member it denies is asked all the
  same, so its node decides (Regulation (EU) 2025/327 Art 11(5); N26,
  N27). The access record names each such member in an
  `ehds-consent-set-aside` detail of its `ehds-emergency-access` entity,
  and the answer never shows it. `"pass-to-node"` needs a declared
  emergency purpose to load, `config check` notes it beside
  `[nl_gf.mitz]`, and `OPTIONS {base}/` declares the setting as
  `federation.consent.emergency`. `crates/ehds-logging` 0.0.13 carries
  `Emergency::with_consent_set_aside`. The book states that the pre-filter
  is asked with the configured `TREAT` or `COC`, never the caller's
  purpose.
