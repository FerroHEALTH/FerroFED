- Information a person inserts, marked as theirs (#992, Regulation (EU)
  2025/327 Art 5): a document the FHIR face receives under a person's own
  grant, a token whose `sub` names the person and whose `patient/` scopes
  are confined to them, is written with every entry naming the record's
  subject, a `PARTY_SELF` with no identifier, as its `provider` (openEHR
  RM `ENTRY.provider`). Its access record carries the
  `ehds-subject-inserted` entity. A person's document that maps to no
  entry, or to a `GENERIC_ENTRY`, is `422` with nothing sent. A
  professional's receipt is unchanged. `eehrxf` 0.0.17 adds
  `ReceivedComposition::inserted_by_subject`, and `ehds-logging` 0.0.22
  adds `AccessRecord::subject_inserted` and its BALP entity.
