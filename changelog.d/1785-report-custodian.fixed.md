- Every report the FHIR face writes from a member's composition, the
  laboratory, discharge and imaging report alike, names that member's
  `Organization` as its `Composition.custodian`, so an imaging report no
  longer carries the member unreferenced, which FHIR R4 documents.html
  §3.3.1 refuses; a written imaging report is held to
  `BundleReportEuImaging` over the Imaging Report package closure, which the
  package set now pins (`eehrxf` 0.3.2,
  [#1785](https://github.com/FerroHEALTH/FerroFED/issues/1785)).
