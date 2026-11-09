- The FHIR face receives a discharge report (Regulation (EU) 2025/327 Art
  14(1)(f)): a `Discharge-Reports` `[[fhir.receive]]` entry holds each
  document to the HL7 Europe Hospital Discharge Report 1.0.0 profiles over
  the package's dependency closure, maps it with the HDR project `eehrxf`
  ships over FerroBRIDGE's ADL 2 discharge report template, and writes it to
  the declared member by its own `ehr_id` alone, with its access record. A
  `[[fhir.receive]]` entry takes an ADL 2 `template` with its `archetypes`
  beside an OPT 1.4 (#1714).
