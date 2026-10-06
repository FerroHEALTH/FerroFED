- An issuer can be declared a national contact point,
  `[auth.issuer.national_contact_point]` (Implementing Regulation (EU)
  2026/2099 Art 6 and 7; #734). A patient-data request from it needs every
  attribute of the 2026/2099 Annex Tables 1 and 2, read from the IHE IUA
  claims where they coincide and from the claims the table names otherwise,
  and a purpose of use whatever `auth.purpose_of_use.required` says; one
  that lacks either is `403 contact-point-attributes-required` or
  `403 purpose-of-use-required`, with nothing sent. Its requests are served
  with consent exclusions withheld whatever `[federation.consent] disclose`
  says (Regulation (EU) 2025/327 Art 8, Art 11(5)). The professional and the
  provider reach each node in the `national_contact_point` claim of the
  conveyance and the access record in an `ehds-relayed` entity, both marked
  as asserted by the contact point, and the outbound gate reads them like
  every other caller claim; the IUA `person_id` is never read. A correlation
  identifier the connector sends in the declared `correlation_header` is
  recorded with the request, and one out of form is
  `400 correlation-invalid`. `OPTIONS {base}/` declares the setting under
  `federation.national_contact_point`, and the book's authentication and
  §13.4 pages name who authenticates the foreign professional (CP-39).
- `crates/ehds-logging` 0.0.6: `Accessor` carries what a contact point
  relays (`Relayed`), `Request` a correlation identifier, and the BALP
  record writes both (#734).
