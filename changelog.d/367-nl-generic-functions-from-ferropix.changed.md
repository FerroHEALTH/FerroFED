- `nl-generic-functions` comes from crates.io at `=0.0.25`, published by
  FerroHEALTH/FerroPIX, which owns the crate from here on; the local copy
  under `crates/` is gone. Its Nuts grant now holds the authorization
  server's issuer to the RFC 8414 rule of `ferrohealth-smart`, so a plain
  `http` issuer is accepted on a loopback host only. The NVI, Mitz, LRZa and
  Nuts behaviour is otherwise unchanged.
