- The access log: every federated query, stored-query execution, routed
  read and routed write that reaches patient data at a node is recorded
  with the verified caller, as Regulation (EU) 2025/327 Annex II 3.2 asks
  of an EHR system's logging component and the GDPR asks of an access
  trail (#623). Each record is a BALP `AuditEvent` sent through the
  `[audit]` spool to the Audit Record Repository. It names the provider,
  the person and their purposes of use, the patient and each `ehr_id`, the
  time, each endpoint asked with its node and outcome, and the categories
  of the data. The categories come from a map you declare in the new
  `[access_log]` table, from template and archetype ids to the Art 14(1)
  categories, national categories or `none`. An access the map cannot
  classify exactly is recorded `unclassified`, with its ids as evidence,
  and is never refused. The gateway stores the record before the answer
  leaves. An access whose record cannot be stored answers
  `503 access-unrecorded` with none of the data. A query run from the
  operator console is recorded with the operator as the caller. The book's
  audit page describes the record and gives an Annex II 3.2 checklist.
- `crates/ehds-logging` 0.0.1, the European logging component as a library
  (#623): the Annex II 3.2 access record, the Art 14(1) categories, the
  category map and its classification, the sink trait, and, behind
  `balp`, the record written as a BALP `AuditEvent` through `ihe-iti`. It
  depends on nothing in FerroFED and on no interoperability component.
