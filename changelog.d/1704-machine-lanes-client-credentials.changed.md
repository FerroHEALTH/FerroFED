- The admin listener's scrape and the PMIR identity feed admit a
  client-credentials token (RFC 6749 §4.4) of a client `metrics.scrape_clients`
  or `pmir.feed_clients` names, which `ferrohealth-smart` classifies as a
  system's, and answer any other request `401` with a `Bearer` challenge, or
  `403` for another client's or a person's token (#1704).
- A token `ferrohealth-smart` classifies as a system's, or that only a
  `system/` scope covers, reaches no patient data whatever professional it
  names, and the assurance level of every caller is the one
  `ferrohealth-smart` assessed under its issuer's `[auth.issuer.assurance]`.
  The Mitz consent pre-filter reads who asks from the NL fields of a
  domestic `federation` issuer's `[[auth.federation]]` table (#1704).
