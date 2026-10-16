- The document list on the FHIR face no longer answers `200` when the
  cross-reference could not answer for a member (#1104): under
  all-or-nothing it fails with `424` naming that member as `not-resolved`,
  as the patient summary and the prescription search do, and under
  `partial` it names the member in an `outcome` entry beside the documents
  it lists.
