- A node and the access record are told only what the eHDSI assertion
  carries of a contact point's caller: the professional's identifier,
  issuing authority, role and `on-behalf-of`, the provider's identifier and
  issuing authority, and the country. No name and no address is conveyed or
  recorded; the record names the professional by their identifier and adds
  the assertion's point of care, facility type, home community,
  authentication class and assertion ids (#1632).
- A contact point's token must be bound to the client certificate of its
  connection (RFC 8705 §3, `sender_constraint = "required"`), and a token
  that falls short of the profile is refused on every route, the
  self-description included. `OPTIONS {base}/` names, per contact point,
  the claim each field is read from by its `ferrohealth-smart` field name
  (#1632).
