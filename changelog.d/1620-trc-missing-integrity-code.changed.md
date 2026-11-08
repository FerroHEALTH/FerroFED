- A national contact point's patient-data request whose token names no
  patient of a treatment relationship confirmation (`trc-missing`) now
  carries `ERROR_SEC_DATA_INTEGRITY_NOT_ENSURED` (MyHealth@EU Requirements
  Catalogue 10.0.0 requirement 11.01.02) in its ITS-REST `ncp_error` and
  its FHIR `issue.details`, where it carried `ERROR_SEC_GENERIC`: the TRC
  patient is a missing assertion attribute, and the family answers one way
  (FerroHEALTH/FerroEHR#5102). A national connector that mapped
  `trc-missing` by its code maps the new one.
