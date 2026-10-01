<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the IHE PIXm FHIR package

Vendored verbatim by `scripts/vendor/ihe-pixm.sh`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://packages.fhir.org/ihe.iti.pixm/3.1.0>, the FHIR package registry's copy of the IG published at
  <https://profiles.ihe.net/ITI/PIXm/3.1.0/>
- Pin: package `ihe.iti.pixm` version `3.1.0`, tarball sha256 `19e2e8eaf3030ac7b4d809c5e1eeb8face02c8635318aeb6d35bc2bb889de0d0`
- Fetched: 2026-10-01
- Upstream licence: Creative Commons Attribution 4.0 International
  (`CC-BY-4.0`, the `license` of the package manifest, listed under What is
  left out;
  <https://creativecommons.org/licenses/by/4.0/>). The package ships no licence
  file of its own. Attribution: IHE International, IT Infrastructure Technical
  Committee, *Patient Identifier Cross-referencing for Mobile (PIXm)* 3.1.0.
- FHIR version: 4.0.1
- Layout: the upstream paths inside the package, unchanged
- Files: 11 of the package's 59, listed below
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `a432e138cf7845288b5259b75294b1300d386933c7b5bf6489d66181a9053d13`
- Read by: #42 (the ITI-83 client of `crates/ihe-iti`, whose tests decode the
  examples and hold the request and response to the OperationDefinition)

## What is here

The artefacts of ITI-83, Get Corresponding Identifiers: the `$ihe-pix`
OperationDefinition with its in and out parameters, the Query Parameters In
and Out profiles, the Consumer and Manager capability statements, and the
IG's examples of an ITI-83 request, a response and the not-found error. The
package's other files serve actors and transactions the client does not play:
the BALP audit profiles, the ITI-104 Patient Identity Feed and its Patient
profiles, the Schematron renderings, the OpenAPI renderings and the registry's
`.index.db`, a SQLite file. They are not taken.

| File | sha256 |
|---|---|
| `package/CapabilityStatement-IHE.PIXm.Consumer.json` | `0c07a1bb7e2998bc2eecacce87556c7592cca60ea81102a270afb1d06eb5c3cf` |
| `package/CapabilityStatement-IHE.PIXm.Manager.json` | `d377fc71a000175a78d30cbd40cb26811ee82ce2f86d8751bf6d3a906cdbe530` |
| `package/ImplementationGuide-ihe.iti.pixm.json` | `63cfb8127b3767bdcf9bb0578eb7626f1b2148d56889c8139d89f81b2755e2d4` |
| `package/OperationDefinition-IHE.PIXm.pix.json` | `d16827774e5f74fc5b53613963d8dc80cc935f898458db5a5d67b50b86ee8ad6` |
| `package/StructureDefinition-IHE.PIXm.Query.Parameters.In.json` | `b545a09c2fa53b68241b739a9252af1f6fda80d517e2915036925a86a720ae7f` |
| `package/StructureDefinition-IHE.PIXm.Query.Parameters.Out.json` | `3631ba3f4236026ad29a52e560bc54ea9ff52c63c2731255b655df29fa02eda0` |
| `package/example/OperationOutcome-pixm-response-error-not-found.json` | `6d12d0aff48a3672b7a859a33b0df7df0defd74c3b875a9af5a546b1b00f1e47` |
| `package/example/Parameters-pixm-request-mohralice-red-all.json` | `9957ade63bcbc551356f49cc90ddc318dbfe2c2d8f4d9f52dee181ab8a540758` |
| `package/example/Parameters-pixm-request-mohralice-red-to-blue.json` | `83499a196da8538c9f2d16c71d0598565c2a5c7f1c7277c88f78509394330fb3` |
| `package/example/Parameters-pixm-response-mohralice-red-all.json` | `1847411fe0bd952528840c278b117b485636469ed4c69b3be0ded0d991ae615c` |
| `package/example/Parameters-pixm-response-mohralice-red-to-blue.json` | `97f664e1c4eca6a9d25168b0d6ffe249167d3496d3f6ea483a650c211b6b5533` |

## What is left out

The package manifest, `package.json`. The script reads its name, version
and licence from the tarball and checks them against the pin. A vendored copy
would make this repository's dependency graph claim an npm package that
depends on `hl7.fhir.r4.core`, a FHIR registry package whose name the GitHub
advisory database flags as a malicious npm package; nothing here installs
either.

| File | sha256 |
|---|---|
| `package/package.json` | `2f1692aabdbb2d5a60650e7d529a341dac8de1668049bd581683394225d9113b` |
