- The gateway reads at most `federation.max_node_answer_bytes` (16 MiB by
  default) of one answer from a member or its token endpoint (#710). A
  longer answer, or one that never ends, is dropped unread, so a faulty or
  hostile member can no longer make the gateway buffer an unbounded body on
  every request. The member is `node-error` with an `error` naming the
  bound, which fails the query `424` under all-or-nothing and clears
  `meta.federation.complete` (§11.1, §11.4); a routed request answers `424`.
  `config check` refuses a zero, and a reload that changes the bound logs it
  as needing a restart.
