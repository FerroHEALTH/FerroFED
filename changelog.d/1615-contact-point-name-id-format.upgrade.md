- Have a national contact point's authorization server write the format of
  the HP Identity Assertion's `NameID` as `ehdsi.assertion.name_id_format`,
  one of the three formats eHDSI SAML Profile 10.0.0 §2.1 allows.
  `ferrohealth-smart` 0.9.4 refuses a contact-point token without it, as it
  refuses one without the `NameID`
  ([Authentication](https://ferrofed.eu/docs/operate/authentication.html), #1615).
