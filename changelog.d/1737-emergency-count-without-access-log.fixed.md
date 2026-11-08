- A development gateway with no access log (`[audit] destination = "off"`)
  now counts a national contact point's emergency accesses against
  `[emergency]` and denies them past the limit (eHDSI Section II Security
  Services 10.0.0 §2.5), where it granted every one uncounted. Production
  always keeps an access log and was not affected.
