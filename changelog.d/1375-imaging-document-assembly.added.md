- `eehrxf` writes the imaging report document, a `BundleReportEuImaging` of
  HL7 Europe Imaging Report 1.0.0-ballot with the `DiagnosticReportEuImaging`
  beside its composition. The composition's type is the report's code, both
  carry the imaging-report and `Medical-Imaging` categories, and its
  organization authors are the report's performing organizations, as the
  profile's `dr-comp-author-org` requires, with the operator organization as
  custodian (#1375).
