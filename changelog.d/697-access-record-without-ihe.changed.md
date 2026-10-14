- The access record needs no regional binding: `[audit]`, its spool and the
  BALP access record are part of every build of `ferrofed-server`, so a
  build without the `binding-ihe` feature records every access to patient
  data and runs a registry outside development. `binding-ihe` keeps only
  the IHE transactions' own records, and the audit spool metrics are on the
  metrics surface in every build (#697).
