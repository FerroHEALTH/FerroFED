- Every PIXm, PDQm, PMIR, mCSD and XCPD client, and the NVI and Mitz
  clients, is built from `ferrohealth-smart`'s outbound client, through
  `ihe-iti` and `nl-generic-functions` 0.2.0, one client per XCPD responding
  gateway. A service that refuses a token with `401` is sent the call once
  more with a fresh token, where the call failed before; the NVI's Nuts
  grant rides on the outbound client as the product's authorization, and a
  MyHealth@EU responding gateway's client is built over the gated TLS of its
  revocation check, so it still sends nothing before the check clears the
  chain (#1815).
