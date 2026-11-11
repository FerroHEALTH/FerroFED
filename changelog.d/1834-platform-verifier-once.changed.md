- Configuration resolution builds the platform's TLS verifier once per
  process for each set of extra roots, through `ferrohealth-server` 0.8.4
  with `ferrohealth-config` 0.4.4 and `ferrohealth-secret` 0.2.1, so a load,
  a reload or `config check` no longer reads the system root store once per
  outbound table. On Linux each read took about 25 ms (#1834).
