- The audit trail and the access record come from the FerroHEALTH audit
  library, FerroAUDIT: the ATNA ITI-20 sender, the BALP `AuditEvent` with
  its FHIR Feed, and the European logging component are the published crates
  `ihe-atna` and `ehds-logging` 0.1.0, pinned exactly, and their local copies
  in `crates/ihe-iti` and `crates/ehds-logging` are gone. What the gateway
  records, where it sends it and how it spools it are unchanged. The log
  events of the spool and the forwarder now carry the targets
  `ihe_atna::atna::spool` and `ihe_atna::atna::forwarder` in place of
  `ihe_iti::atna::spool` and `ihe_iti::atna::forwarder`.
