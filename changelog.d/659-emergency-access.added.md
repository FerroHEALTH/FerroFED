- An access to patient data in the vital interests of the data subject is
  marked in its access record (Regulation (EU) 2025/327 Art 11(5)): each
  `[[access_log.emergency_purpose]]` names a purpose of use, such as the
  HL7 v3 `ActReason` code `BTG`, and a record whose verified token declares
  one carries the entity `ehds-emergency-access`, with a description in
  words and the purposes that marked it. The mark is read from the token's
  purposes alone and never inferred. It changes no dispatch and no answer:
  the purpose reaches each node in the conveyance, the node decides, and
  a node's refusal stands. The gateway writes one log line under its
  request id for each marked access, and `config check` notes an access
  log that names no emergency purpose. The book's audit and authentication
  pages name the codes a deployment maps (#659).
- `crates/ehds-logging` 0.0.9: the `emergency` module, `EmergencyPurposes`
  and `Emergency`, the record's `emergency`, and its BALP entity (#659).
