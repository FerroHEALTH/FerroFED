- `[trust.issuer.national_contact_point]` is removed, and a configuration
  that still sets it is refused, naming its successor. Declare the contact
  point as an `[[auth.issuer]]` of `kind = "federation"` with
  `sender_constraint = "required"`, an `[[auth.federation]]` table with
  `contact_point = true`, `mapping = "ehdsi"`, its `countries` and the
  constant `professional.country_code`,
  `professional.issuing_authority_name` and
  `provider.issuing_authority_name`, and a `[[federation.contact_point]]`
  naming the issuer, with `correlation_header` and `additional_countries`
  moved there. Have the contact point's authorization server write the
  assertion's attributes under `ehdsi`, the TRC patient as
  `ehdsi.patient.id`, and bind the token to the connector's client
  certificate ([National contact point](https://ferrofed.eu/docs/integrate/national-contact-point.html),
  #1632).
