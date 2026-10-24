- The interoperability component asks its terminology server
  (`[eehrxf.terminology]`) through FerroBRIDGE's published
  `ferrobridge-term` 0.1.131 in place of FerroFED's own client: the same
  `$translate`, `$lookup` and `$validate-code` in FHIR R4, over the same
  credential and TLS material, mutual TLS and private trust roots included.
  A code the server does not know is now named as such rather than by the
  HTTP status it came with, and every other refusal still carries the
  server's status. `eehrxf` 0.0.28 changes its terminology seam:
  `TerminologyServer` takes the system, code, target, value set and
  candidate it asks about and answers the matches, the display or the
  result, in place of the operations' whole `Parameters`, and
  `TerminologyError` gains `NotFound`; an implementation of the seam
  outside FerroFED changes with it
  ([#925](https://github.com/FerroHEALTH/FerroFED/issues/925)).
