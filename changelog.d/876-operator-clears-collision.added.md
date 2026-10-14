- The operator can clear a held `ehr_id` collision once the node at fault
  has fixed it, with `POST /admin/ehr-ids/{ehr_id}/clear` on the admin
  listener (#876). It needs a token with the issuer's `operator_scope`, is
  recorded with the operator who ran it, takes the node-local `ehr_id`
  alone, and forgets it from this replica's `ehr_id` index and from every
  caller's resolution bindings. The next read asks every member again and
  is refused `409` again while the collision stands; a write is never probed
  for. The answer says what the replica held, and the incident report lists
  each cleared collision under `cleared`.
