<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the IHE PDQm FHIR package

Vendored verbatim by `scripts/vendor/ihe-pdqm.sh`. Never edit a file here:
change the pin in docs/VERSIONS.md and re-run the script.

- Source: <https://packages.fhir.org/ihe.iti.pdqm/3.2.0>, the FHIR package registry's copy of the IG published at
  <https://profiles.ihe.net/ITI/PDQm/3.2.0/>
- Pin: package `ihe.iti.pdqm` version `3.2.0`, tarball sha256 `61e09fbee991ff7c131b6ba5474921001e07782209961f3e85cee5f3ebcaedc2`
- Fetched: 2026-10-02
- Upstream licence: Creative Commons Attribution 4.0 International
  (`CC-BY-4.0`, the `license` of the package manifest, listed under What is
  left out;
  <https://creativecommons.org/licenses/by/4.0/>). The package ships no licence
  file of its own. Attribution: IHE International, IT Infrastructure Technical
  Committee, *Patient Demographics Query for Mobile (PDQm)* 3.2.0.
- FHIR version: 4.0.1
- Layout: the upstream paths inside the package, unchanged
- Files: 8 of the package's 53, listed below
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `14dc6eee3086cda0a4f54e181a77f65fa3abc40f1c1f4aec0636d3a58e3f2cb9`
- Read by: #119 (the ITI-78 client of `crates/ihe-iti`, whose tests hold the
  query to the Supplier's Patient search parameters and decode the example
  response Bundle and Patients)

## What is here

The artefacts of ITI-78, Mobile Patient Demographics Query: the Patient
Demographics Consumer (Query) and Supplier capability statements, which list
the Patient search parameters a Supplier processes, the Query Patient Resource
Response Message profile of the `searchset` Bundle, the PDQm Patient profile,
the ImplementationGuide, and the IG's examples of a response Bundle and two
Patients. The package's other files serve no reader here: the ITI-119
`$match` OperationDefinition, its parameter profiles, capability statements
and examples, the BALP audit profiles and examples, the XML renderings, and the
registry's validation output. They are not taken.

| File | sha256 |
|---|---|
| `package/CapabilityStatement-IHE.PDQm.PatientDemographicsConsumerQuery.json` | `c32bc60f13e95091f4564b62ce037d55a2d827b4e58120229f8ad7ed29e32c4d` |
| `package/CapabilityStatement-IHE.PDQm.PatientDemographicsSupplier.json` | `b5a57396695b4ada78474d47e2a77b6d7f82f79a89a3f9d3ff4735fda885406f` |
| `package/ImplementationGuide-ihe.iti.pdqm.json` | `05acdb5bf6b8e021b99b8fc845484c938dd3e2c4abba4e0c9b836979065c0c5f` |
| `package/StructureDefinition-IHE.PDQm.Patient.json` | `d184b3cfb58b91eecb87e6ee865c7108a779a2efe942a580c166776d8d704549` |
| `package/StructureDefinition-IHE.PDQm.QueryPatientResourceResponseMessage.json` | `0488cf89754f6c914463789758919bec63f3ce297dc6d11088c0eafe0f31f590` |
| `package/example/Bundle-ex-QueryPatientResourceResponseMessage.json` | `851a55358466f91e261ee90c3f03c3de605679f1b3d8abe19d082bf8da6a9f39` |
| `package/example/Patient-ex-patient-mothers-maiden-name.json` | `3b8396d82d2cb858c0b2124d75809156ca4f516b8c74636098201f64546a3e09` |
| `package/example/Patient-ex-patient.json` | `83f68fd6bf11e0efd8c12bea2e4536c4415c1ee77f22cfbc91cc30768d6f1014` |

## What is left out

The package manifest, `package.json`. The script reads its name, version
and licence from the tarball and checks them against the pin. A vendored copy
would make this repository's dependency graph claim an npm package that
depends on `hl7.fhir.r4.core`, a FHIR registry package whose name the GitHub
advisory database flags as a malicious npm package; nothing here installs
either.

| File | sha256 |
|---|---|
| `package/package.json` | `fbd6988ebb77e8fed14590e874704b4a775a2470e08b4673c9c6bdb1f6dd4288` |
