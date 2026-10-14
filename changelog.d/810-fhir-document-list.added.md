- The document list on the Annex II 2.1 FHIR face (#810):
  `GET {fhir-base}/DocumentReference?patient.identifier=system|value`, with
  the optional `type` token of the patient summary, LOINC `60591-5`, answers
  a `searchset` of `DocumentReference`s that meet the HL7 Europe EU Health
  Data API `EehrxfMhdDocumentReference` profile: the summary the gateway
  writes on request, retrievable through `Patient/$summary`, and each
  `native` member's own document, named by `attachment.url`
  `Binary/{token}` with its media type, SHA-1 hash and size. The gateway
  asks each such member's own endpoint by its `ehr_id` alone, through the
  outbound identifier-hygiene gate, with the same budget, onward credential
  and signed caller identity as for a summary, and runs no section query. A
  member that does not answer fails the list, `424` or `504`, under
  all-or-nothing, and is named in an `OperationOutcome` entry with
  `search.mode` `outcome` under `partial`. `GET {fhir-base}/Binary/{token}`
  reads a listed document through the gateway: the token is a JWS the
  gateway signs with its `[signing]` key, of its own JOSE type, valid for
  `fhir.document_url_lifetime_secs` (300 unless set) and for the caller it
  was listed for, and carries no patient identifier; the member is asked
  again and its bytes are answered unchanged, with nothing stored. Both
  take the scopes of a summary and write an access record naming every
  member endpoint asked, and `metadata` declares the search and the read.
