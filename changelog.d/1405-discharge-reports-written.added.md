- The FHIR face writes a discharge report from a member's openEHR
  composition: a `[[fhir.report]]` entry declares the template of the
  members' discharge compositions, an OPT 1.4 or an ADL 2 template with its
  archetypes, and the document list names one discharge report per
  composition, typed `18842-5`, read as an HL7 Europe Hospital Discharge
  Report 1.0.0 `bundle-eu-hdr` written by the shipped HDR mapping. The
  reserved stored query `eu.ferrofed.eehrxf::discharge-report` selects the
  compositions. A `Laboratory-Reports` entry takes the place of the
  laboratory template of `[[fhir.receive]]` (#1405).
