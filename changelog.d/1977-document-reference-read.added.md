- Every entry of the document list has a stable id, the same in every list,
  and a member's own `DocumentReference` takes it in place of the member's
  id. `_id` selects entries by it. With `[store]`, `_id` alone and
  `GET {fhir-base}/DocumentReference/{id}`, the MyHealth@EU national contact
  point API's read, answer the document through the store's sealed record of
  its patient, and an id the store no longer holds is a `404` asking for the
  search again
  ([Reading a listed document](https://ferrofed.eu/docs/integrate/patient-summary.html#reading-a-listed-document),
  #1977).
