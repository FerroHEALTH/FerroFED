- Every refusal to a national contact point names its MyHealth@EU
  Requirements Catalogue 10.0.0 §1.25 exception code: in the ITS-REST error
  body's `ncp_error`, in the FHIR `OperationOutcome`'s `issue.details`, and
  at the head of the access record's outcome description. A refused token
  takes the code `ferrohealth-smart` 0.9.2 gives it, the gateway's own
  refusals the code of their catalogue row, and no legacy appendix 6.4
  number is issued (#1633).
