- The electronic prescription and dispensation datasets of the
  interoperability component (#1038, Regulation (EU) 2025/327
  Art 14(1)(b) and (c)): `eehrxf` 0.0.21 carries, under the features
  `prescription` and `dispensation`, the crosswalks from the Xt-EHR
  `EHDSMedicationPrescription` and `EHDSMedicationDispense` 1.0.0 models to
  the HL7 Europe MPD 1.0.0 `MedicationRequest-eu-mpd` and
  `MedicationDispense-eu-mpd` profiles, held to both vendored packages. With
  `fhir-r4`, the new `medication` module reads a member's
  `MedicationRequest` and `MedicationDispense` into the datasets with a typed
  refusal for every rule a resource breaks, writes them back in the MPD
  form, and checks a resource and each `Medication` it contains against the
  vendored MPD profiles, naming what the check cannot evaluate. The number
  of dispensations a prescription item has left travels in the extension
  `https://ferrofed.eu/fhir/StructureDefinition/remaining-dispensations` on
  `dispenseRequest`, written only when the member states a value.
