- A patient the cross-reference could not answer for at any member is no
  longer answered on the FHIR face as a patient no member holds (#1107).
  Under `partial`, the patient summary names each such member as
  `not-resolved` in every section, where it answered `404`, and the
  document list names each one in an `outcome` entry, where it answered an
  empty `searchset`. A patient summary under `partial` now also names a
  single member the cross-reference could not answer for. A patient every
  member answers it does not hold still gets the empty answer.
