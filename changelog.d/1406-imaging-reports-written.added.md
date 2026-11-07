- The FHIR face writes an imaging report from a member's openEHR
  composition: a `Medical-Imaging` `[[fhir.report]]` entry declares the
  template of the members' imaging compositions, and the document list names
  one imaging report per composition, typed `85430-7`, read as an HL7 Europe
  Imaging Report 1.0.0-ballot `BundleReportEuImaging` written by the shipped
  imaging mapping. The reserved stored query
  `eu.ferrofed.eehrxf::imaging-report` selects the compositions (#1406).
