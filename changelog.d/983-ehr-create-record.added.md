- The access record of a routed EHR create names the EHR it left
  (Regulation (EU) 2025/327 Art 9(1)): the `ehr_id` the member's `ETag` or
  `Location` named (ITS-REST 1.1.0 EHR API), held to the `HIER_OBJECT_ID`
  form, is an `ehr` entity with the patient the identity binding holds
  behind it, and the origin's `created-ehr`; an answer that names none, no
  `ehr_id` or two that differ records `created-ehr-unread` as `unnamed` or
  `malformed` and names no value. `ehds-logging` 0.0.21 adds `CreatedEhr`
  and the origin's `created_ehr` (#983).
