- A national contact point's token that carries what a conforming eHDSI HP
  Identity Assertion provides is admitted. The gateway no longer asks for
  the ten Implementing Regulation (EU) 2026/2099 Annex fields, which the
  assertion does not carry: `ferrohealth-smart` 0.9.2 holds the token to
  the MyHealth@EU contact-point profile (eHDSI SAML Profile 10.0.0 §2.1,
  §2.3), the professional is named by the assertion's `NameID`, and a
  purpose of use other than `TREATMENT` or `EMERGENCY`, or a token missing
  the facility type, the home community, the point of care, the
  authentication class or `on-behalf-of` for an ISCO `44` role, is refused
  `403 contact-point-attributes-required` (#1632).
