- Each resolution binding now expires `federation.binding_ttl_ms` after the
  last resolution that returned it. A caller that kept resolving used to keep
  every binding it had ever recorded, so a binding made stale by an identity
  merge or split could route a follow-up to a node the patient no longer maps
  to for as long as the caller stayed active (#650).
