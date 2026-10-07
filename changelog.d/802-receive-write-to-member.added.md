- Receiving a document in the exchange format (Regulation (EU) 2025/327
  Annex II 2.2 and 2.3, #802): `POST {fhir-base}/Bundle` takes a document
  `Bundle` and writes it as one openEHR composition to the member
  `[[fhir.receive]]` declares for its Art 14(1) category, by ITS-REST
  `composition_create` at that member's own `ehr_id`. The document is held
  to the category's HL7 Europe profiles from the package the deployment
  supplies, mapped by its FHIRconnect mapping, and kept whole in the
  composition's `FEEDER_AUDIT.original_content`. Its patient is resolved
  from every `Patient.identifier` in a namespace `fhir.receive_namespaces`
  names, at the receiving member alone, and every identifier must name the
  same `ehr_id`; no patient identifier is sent in the path, the query string
  or the headers. The caller needs `user/composition-*.c` (a read scope is
  refused), and a `patient/` grant writes only into its own patient's EHR.
  A category with no member, a document that breaks its profiles, a patient
  the member holds no EHR for, identifiers that name two patients, and a
  member's refusal or silence are each an `OperationOutcome` error, never a
  success; every receipt that reached the member writes one access record.
  A member the registry does not hold, or receiving with no resolver,
  refuses the start and `config check`.
- `eehrxf` 0.0.8: `ReceivedDocument::category` names a received document's
  priority category from its `Composition.type`, `Category::profiles` and
  `Category::document_type` name the profiles and the document type of each
  category, `Category::code` and `Category::of_code` name each category by
  its HL7 Europe priority category code, such as `Patient-Summaries`, and
  `ReceivedComposition::model` is the mapped composition as the RM model
  holds it (#802).
