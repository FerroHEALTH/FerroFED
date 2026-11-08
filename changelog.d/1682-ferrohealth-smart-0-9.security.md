- A Basic user never holds a `system/` scope, through `ferrohealth-smart`
  0.9.1 (GHSA-jhg2-jwjp-hq74, CWE-269, CWE-863). Before it, an
  `[[auth.basic.users]]` entry granted a `system/` scope gave that user
  every resource of a type, the reach of a backend service. Such an entry
  now refuses the start, naming the user and the scope. Upgrade step: this
  release removes `[[auth.basic.users]]` altogether (#1704, under Removed),
  so move a person on Basic to a `persons` issuer, and a machine to a
  client credentials client of a `machines` or `persons` issuer (RFC 6749
  §4.4).
