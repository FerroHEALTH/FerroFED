- A `native` member's answer on the FHIR face counts as its document only
  when it is a FHIR document `Bundle` whose `identifier` has a system and
  a value (FHIR R4 `bdl-9`), which the `EehrxfMhdDocumentReference`
  profile needs as the list entry's `masterIdentifier` (#901). Any other
  answer, a document with no identifier included, makes the member
  `node-error`. Under all-or-nothing, `Patient/$summary` and the document
  list fail with `424` and name the member. Under `partial`, the summary
  names it as a member that did not answer, never as one covered by its
  own document, and the list leaves its answer out and names it in an
  `outcome` entry. `GET {fhir-base}/Binary/{token}` refuses such an answer
  with `424` and returns none of its bytes. The access record counts no
  document the answer did not deliver.
