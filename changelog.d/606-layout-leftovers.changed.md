- `ferrofed-identity` names the PMIR subscriber's error `ihe::pmir::PmirConfigError`,
  after its module, where it was `LifecycleConfigError`, and the log target both
  audit log recorders write to is `ihe::audit::AUDIT_TARGET`, where it was
  `ihe::xcpd::AUDIT_TARGET`; its value, `ferrofed::audit`, is unchanged. In
  `nl-generic-functions` 0.0.17, `mitz::BSN_ROOT` takes its value from the BSN
  OID of `identification::BSN_SYSTEMS`, which is unchanged (#606).
