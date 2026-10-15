- Information a representative inserts, marked as theirs (#1003,
  Regulation (EU) 2025/327 Art 4(2) and Art 5): an issuer that sets
  `act_is_representative = true` on `[auth.issuer.patient]` declares that
  the RFC 8693 `act` claim of its patient tokens names a proxy or a legal
  representative. A document the FHIR face receives under such a token is
  written with every entry naming the representative as its `provider`, a
  `PARTY_IDENTIFIED` whose one identifier is the `act` `sub`, never the
  person's `PARTY_SELF`, and its access record carries the
  `ehds-representative-inserted` entity. A token whose `act` names the
  representative by nothing, by its own `sub` or by an identifier of the
  document's patient is `403` with nothing sent. Without the declaration
  the gateway reads no representative, and the person's own insertion
  stays `PARTY_SELF`. `eehrxf` 0.0.19 adds `Representative` and
  `ReceivedComposition::inserted_by_representative`; `ehds-logging` 0.0.24
  replaces `AccessRecord::subject_inserted` with `inserted`, an
  `Insertion` of the subject or a representative. RFC 8693 is vendored.
