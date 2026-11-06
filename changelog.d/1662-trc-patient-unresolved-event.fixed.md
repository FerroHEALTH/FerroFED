- A national contact point's TRC patient that the cross-reference cannot
  resolve is logged as its own security event, `trc-patient-unresolved`,
  naming the treatment relationship confirmation, on the ITS-REST and FHIR
  faces; it was logged as `patient-context-unavailable`, the event of a
  `patient/` grant. The answer stays `424 patient-context-unavailable`. The
  metrics page lists every security event (#1662).
