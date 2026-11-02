- The gateway implements the Federation Tier with AQL 1.0.1, the erratum
  to the routing of EHR-scoped version requests (§12.3, §12.4, §12.5.1,
  N22, N23, N36, N41). A request under `{base}/v1/ehr/{ehr_id}/…`, a
  version read or a versioned write, is located by the targeting header,
  the caller's binding and the `ehr_id` index as before, and now, when
  none of them names a node and the path carries a version uid, by the
  member whose `system_id` is that uid's `creating_system_id`; an
  untargeted read of such a version reaches its creator without an
  ask-all probe, and a composition `DELETE` whose version a member
  created reaches that member where it was a `400 target-required`. A
  read that reaches a node holding an imported copy returns the copy with
  `openEHR-federation-system-id` naming that node, and a write that
  reaches it is refused `409` naming the controlling system and is never
  redirected. `OPTIONS {base}/` still reports `spec_version` `"1.0"`.
