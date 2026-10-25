- `eehrxf` writes the laboratory report and the discharge report documents.
  The laboratory report is a `Bundle-eu-lab` of HL7 Europe Laboratory
  Report 2.0.0, with the `DiagnosticReport-eu-lab` beside its composition;
  the two hold one identifier, type, subject and category and each names
  the other, and the results go in sections coded by study type (#1355).
  The discharge report is a `bundle-eu-hdr` of HL7 Europe Hospital
  Discharge Report 1.0.0, with the encounter it reports on and its language
  (#1357). The patient summary and both reports share one assembly,
  `document::Form`.
