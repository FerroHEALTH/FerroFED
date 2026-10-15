- Under `config.ui = "managed"`, the operator console manages each onward
  credential and each binding section (`[pixm]`, `[pdqm]`, `[xcpd]`,
  `[nl_gf]`, `[federation.localization]`, `[federation.consent]`) as one
  resource owned whole (#578): one the file sets is read-only and refused
  `409 resource-file-owned`, and the console's survive a `SIGHUP` reload.
  `PUT /admin/config/credentials/{endpoint_id}` and
  `PUT /admin/config/bindings/{section}` save a revision with the resource's
  settings and secret changes; `…/remove` saves one without it. A revision
  holds a secret only as its `_file` reference: a secret value is written to
  a new file of mode `0600` in the new `config.secrets_dir`, never logged,
  echoed or answered, and refused `409 secret-store-unavailable` without
  that directory. An applied change to a credential or a binding section
  awaits a confirmation, `POST /admin/config/revisions/{id}/confirm`, within
  the new `config.confirm_ms` (120000 ms by default); without one the
  gateway rolls it back, on every replica and across a restart. The new
  `config.approval` names resources a revision changes only once an
  operator other than its author approved it,
  `POST /admin/config/revisions/{id}/approve`. Client authentication stays
  in the file. The Configuration page shows each credential and binding
  section with its owner and its secrets by reference, and offers the
  confirmation and the approval with their deadline and who acts.
