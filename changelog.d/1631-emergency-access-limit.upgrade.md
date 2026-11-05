- The store applies migration 2, which adds `ferrofed.emergency_access`, at
  the first start. A deployment that declares a national contact point and
  names its emergency purpose in `[[access_log.emergency_purpose]]` now
  denies a professional's sixth emergency access in 24 hours; set
  `[emergency]` to tighten the limit, never to loosen it (#1631).
