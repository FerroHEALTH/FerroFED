- The FHIR R4 `StructureDefinition` of the remaining-dispensations extension
  (#1053): `eehrxf` 0.0.23 carries it as
  `medication::prescription::REMAINING_DISPENSATIONS_DEFINITION`, a simple
  extension of `MedicationRequest.dispenseRequest` with a
  `valueUnsignedInt`, at most once, and the documentation site serves it at
  `https://ferrofed.eu/fhir/StructureDefinition/remaining-dispensations` and
  beside it as `remaining-dispensations.json`. It is tested against the
  FHIR R4 4.0.1 Extensibility, Defining Extensions and StructureDefinition
  pages, now vendored by `scripts/vendor/fhir-r4.sh`.
