<!-- This file describes vendored material; the bytes beside it keep the
     terms named below, not the licence of this repository. -->

# Provenance: the FerroPULSE report envelope v2

- Source: <https://github.com/FerroHEALTH/FerroPULSE> (private; Cadasto B.V., the Licensor
  of this repository too)
- Pin: commit `f83df34cc968f5e0f916834508336c8eeb4c9e41`
- Schema sha256: `e609f17a5bfa78c6c08bdd3a28025889e20e0fde5c72fa881e7ed47b6abc8bc7`
- Fetched: 2026-11-02 by `scripts/vendor/ferropulse.sh`
- Licence: upstream declares `LicenseRef-Proprietary` in its `Cargo.toml`
  and ships no LICENSE file; Cadasto B.V. holds the copyright of both
  repositories and publishes this copy under this repository's licence
- Files: 69, copied verbatim: `docs/report-v2.md` (the contract),
  `schemas/report-envelope.v2.json` (JSON Schema 2020-12) and `schemas/examples/report-envelope.v2/`
  (the contract's own valid and invalid examples)

The usage report tests in `app/ferrofed-server/tests/it/usage_report.rs`
check the validator against the examples, then validate every report the
server builds against the schema. Never hand-edit these files; change the pin
in `docs/VERSIONS.md` and re-run the script.
