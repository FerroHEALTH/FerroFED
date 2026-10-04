<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the IHE BALP FHIR package

Vendored verbatim by `scripts/vendor/ihe-balp.sh`. Never edit a file here:
change the pin in docs/VERSIONS.md and re-run the script.

- Source: <https://packages.fhir.org/ihe.iti.balp/1.1.4>, the FHIR package registry's copy of the IG published at
  <https://profiles.ihe.net/ITI/BALP/1.1.4/>
- Pin: package `ihe.iti.balp` version `1.1.4`, tarball sha256 `be46dda3088ee9d486d7163458dc5a4d7192e8a1c587fd47e2beb70e4bcefa90`
- Fetched: 2026-10-04
- Upstream licence: Creative Commons Attribution 4.0 International
  (`CC-BY-4.0`, the `license` of the package manifest, listed under What is
  left out;
  <https://creativecommons.org/licenses/by/4.0/>). The package ships no licence
  file of its own. Attribution: IHE International, IT Infrastructure Technical
  Committee, *Basic Audit Log Patterns (BALP)* 1.1.4.
- FHIR version: 4.0.1
- Layout: the upstream paths inside the package, unchanged
- Files: 9 of the package's 126, listed below
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `081f409ae3f915ac29bca7cf85fed92ed63b9604882f219422b00d050057e342`
- Read by: #486 (the BALP audit records of `crates/ihe-iti`, whose tests hold
  each record to the pattern its transaction's audit profile derives from and
  a search to the client-side example, and the ATNA FHIR Feed sender, held to
  the Audit Creator's `create` interaction)

## What is here

The RESTful audit patterns an IHE transaction's audit profile derives from:
Query and Patient Query (a search), Read, Create and Delete, the Audit
Creator capability statement (an
ATNA Secure Application or Secure Node with the ATX: FHIR Feed Option,
`create` on `AuditEvent`) and the Audit Record Repository capability
statement, the ImplementationGuide, and the IG's client-side example of a
search. The package's other files serve no reader here: the OAuth and SAML
token-use, consent, privacy disclosure and update patterns, the Patient Read,
Create and Delete variants, the other examples, the code systems and value
sets, the
Schematron renderings, the OpenAPI renderings and the registry's validation
output. They are not taken.

| File | sha256 |
|---|---|
| `package/CapabilityStatement-IHE.BALP.ATNA.AuditRecordRepository.json` | `f553800b7cad86c39031908ea91e43f69ecffbafe86a499c7bc794edd62183bb` |
| `package/CapabilityStatement-IHE.BALP.AuditCreator.json` | `206261e7aab7ed84184b396060f2e23727c0609a77c8a5c17f9e9dfc00fbbd8e` |
| `package/ImplementationGuide-ihe.iti.balp.json` | `c025a67383dfb498479af43fb20b71c34b9af7183d44bc1cf7a30bfc6e0c1026` |
| `package/StructureDefinition-IHE.BasicAudit.Create.json` | `5ffa75afb4fcefbb3d08fc0f8a7f801a465ce1f051800617d9a94de50cc57f11` |
| `package/StructureDefinition-IHE.BasicAudit.Delete.json` | `25843f721481d4abcfeb042cd509c342fcb957737700282af65c126a6d681f50` |
| `package/StructureDefinition-IHE.BasicAudit.PatientQuery.json` | `08fde79d720cc1a48118bcce4abbe9bb66f7cdd9cf81c8f51e7adca161978fd6` |
| `package/StructureDefinition-IHE.BasicAudit.Query.json` | `fd5c83e494e9af6da0782cbc0fde18897200bb19f07b21d81644aac0aa34e656` |
| `package/StructureDefinition-IHE.BasicAudit.Read.json` | `569e46ce96921d412d781841f1bd18b316b04294130916fffd99bf7991060e7e` |
| `package/example/AuditEvent-ex-auditBasicQueryGetClient.json` | `c5d73df6074bff3247b1a0215642d2934694c546d45a9a2865f08ca953c31f9b` |

## What is left out

The package manifest, `package.json`. The script reads its name, version
and licence from the tarball and checks them against the pin. A vendored copy
would make this repository's dependency graph claim an npm package that
depends on `hl7.fhir.r4.core`, a FHIR registry package whose name the GitHub
advisory database flags as a malicious npm package; nothing here installs
either.

| File | sha256 |
|---|---|
| `package/package.json` | `82c2985c0bd83becbcd82a7212f42337b48deb2eccf66ef87ee07eebef7b4a58` |
