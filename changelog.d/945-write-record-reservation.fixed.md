- A write whose access record the audit spool has no room for is refused
  `503 access-unrecorded` before any member is asked. Before, a routed
  ITS-REST write or a document received on the FHIR face reached its
  member first and was then answered `503`, so a client that sent it again
  wrote it twice. The gateway now reserves the record's place in the spool
  before the write, and two writes never take its last place. Once a member
  was asked, its answer stands; a reserved record that still cannot be
  stored raises the security event `write-unrecorded` and the alert
  `FerroFEDWriteUnrecorded`. `AccessSink` in `ehds-logging` 0.0.17 gains
  `reserve`, and `ihe-iti` 0.0.35 adds `Spool::reserve` and
  `AuditRecorder::reserve`.
