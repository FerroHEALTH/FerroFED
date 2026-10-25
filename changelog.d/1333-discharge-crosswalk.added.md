- `eehrxf` 0.0.32 carries the discharge report crosswalk under its
  `discharge` feature: each Xt-EHR `EHDSDischargeReport` element with the
  eHealth Network ids of the Guideline on Hospital Discharge Report Release
  1.1, its producer obligation, and the element of the HL7 Europe Hospital
  Discharge Report 1.0.0 composition that carries it, checked against the
  vendored packages. A crosswalk row now admits any context profile where
  the profile element references any resource (#1333).
