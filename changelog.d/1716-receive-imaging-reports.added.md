- The FHIR face receives an imaging report (Art 14(1)(d)): a
  `Medical-Imaging` `[[fhir.receive]]` entry now loads over the HL7 Europe
  Imaging Report 1.0.0-ballot package closure, and a received report is
  held to `BundleReportEuImaging`, mapped by the imaging project `eehrxf`
  ships over FerroBRIDGE's ADL 2 imaging report template, and written to
  the declared member by its own `ehr_id` alone, with its access record
  ([#1716](https://github.com/FerroHEALTH/FerroFED/issues/1716)).
