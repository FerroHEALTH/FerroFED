- The professional, the organisation and the purposes of use a caller's
  token states are read only through its issuer's `[[auth.attributes]]` table
  (a `persons` issuer) or its `[[auth.federation]]` table, as
  `ferrohealth-smart` reads them; the gateway's own reader of the IHE IUA
  extension and the `authorization_details` purposes is gone (#1615).