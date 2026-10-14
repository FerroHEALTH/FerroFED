- `GET {base}/v1/ehr?subject_id=…` whose subject resolves at more than one
  member now answers `400` (`subject-several`) instead of `409`, as N41 has
  it when no one node is found, with the endpoints a client may name in
  `openEHR-federation-endpoint` listed in the message (#869). An `ehr_id`
  two members claim keeps N42's `409` (`ehr-id-collision`). A client that
  branched on the `409` reads the `400` and the `subject-several` code.
