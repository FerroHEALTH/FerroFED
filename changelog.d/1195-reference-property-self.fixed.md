- The interoperability component's reference property admits `#` from the
  Patient entry, which FHIR R4 resolves to the holding resource itself, so
  the test no longer fails when its generator draws that input; the case is
  kept as a regression seed (#1195).
