- If you edited the `ferroehr-config` block of the quickstart's
  `compose.yaml`, FerroEHR 5.3.2 refuses the 4.3.9 Basic user keys at boot:
  rename `username` to `user`, give each user its `scopes` (the quickstart
  grants `user/composition-*.cruds`, `user/template-*.cruds`,
  `user/aql-*.cruds` and `__ferroehr.platform`), and move each user's
  `roles` to `[authz.rbac.basic_roles]` keyed by the user (#1226).
