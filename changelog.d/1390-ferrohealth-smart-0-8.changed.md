- Moved to `ferrohealth-smart` 0.8.6, with `ihe-iti` 0.0.66,
  `nl-generic-functions` 0.0.33, `ihe-atna` and `ehds-logging` 0.3.1,
  `ferrobridge-term` and `ferrobridge-validator` 0.1.140 on the same line,
  so the build holds one `ferrohealth-smart` and one `openehr-sdt` (#1390).
  A verified Basic credential of `[[auth.basic.users]]` is now kept for
  300 seconds by default (`auth.basic.cache_ttl_seconds`; `0` keeps the old
  behaviour), and `[auth] ca_file` names extra trust roots for the issuers.
  An access record names each security label once, on its `ehds-categories`
  and `origin` entities, and writes no `meta.security`, as IHE RESTful ATNA
  Rev. 3.6 Table 3.81.4.2.2.1-1 maps no `meta`: a repository that filtered
  on `_security` reads the entities' `securityLabel` instead.
