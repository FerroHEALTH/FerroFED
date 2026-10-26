- The static development cross-reference `[dev]` (`[[dev.crossref]]` and
  `[[dev.consent_denied]]`) is removed: FerroFED keeps no patient index in
  any profile and resolves every patient through a PIX Manager. A
  configuration that sets `[dev]` is refused at load naming the table. The
  operator deployment report drops `development_crossref`, the identity-state
  export drops its `development_cross_reference` entry, the usage report no
  longer names the `dev` service, and the console's development banner no
  longer mentions a static cross-reference (#1377).
