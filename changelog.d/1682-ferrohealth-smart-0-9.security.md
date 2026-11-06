- A Basic user never holds a `system/` scope, through `ferrohealth-smart`
  0.9.1 (GHSA-jhg2-jwjp-hq74, CWE-269, CWE-863). Before it, an
  `[[auth.basic.users]]` entry granted a `system/` scope gave that user
  every resource of a type, the reach of a backend service. Such an entry
  now refuses the start, naming the user and the scope. Upgrade step: give
  each Basic user the `user/` scopes a person holds, and move a machine on
  Basic to a client credentials client of a `machines` or `persons` issuer
  (RFC 6749 §4.4).
