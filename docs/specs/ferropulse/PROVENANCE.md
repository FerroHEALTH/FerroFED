<!-- This file describes vendored material; the bytes beside it keep the
     terms named below, not the licence of this repository. -->

# Provenance: the FerroPULSE report envelope v2

- Source: <https://github.com/FerroHEALTH/FerroPULSE> (private; Cadasto B.V., the Licensor
  of this repository too)
- Pin: commit `26957066ad7a7dcd5fc38947b6baa8df0c08609d`
- Schema sha256: `09048000b428cbb010d165b52a4579f60672c471f167a3b58410226dddfec9c2`
- Fetched: 2026-10-21 by `scripts/vendor/ferropulse.sh`
- Licence: upstream declares `LicenseRef-Proprietary` in its `Cargo.toml`
  and ships no LICENSE file; Cadasto B.V. holds the copyright of both
  repositories and publishes this copy under this repository's licence
- Files: 68, copied verbatim: `docs/report-v2.md` (the contract),
  `schemas/report-envelope.v2.json` (JSON Schema 2020-12) and `schemas/examples/report-envelope.v2/`
  (the contract's own valid and invalid examples)

The usage report tests in `app/ferrofed-server/tests/it/usage_report.rs`
check the validator against the examples, then validate every report the
server builds against the schema. Never hand-edit these files; change the pin
in `docs/VERSIONS.md` and re-run the script.
