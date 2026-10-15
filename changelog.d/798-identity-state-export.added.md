- The operator can export the routing state a gateway process learned, for
  a system that replaces FerroFED (Regulation (EU) 2025/327 Annex II 2.6),
  with `POST /admin/identity-state/export` on the admin listener (#798). It
  needs a token with the issuer's `operator_scope` and is recorded with the
  operator who ran it. The answer is JSON with a `format_version`, the
  federation id and every learned `creating_system_id` mapping with its
  node and endpoint, documented on the book's "Exporting the identity
  state" page. It leaves out the resolution bindings and the `ehr_id`
  index, so it carries no patient identifier; each replica is exported on
  its own.
