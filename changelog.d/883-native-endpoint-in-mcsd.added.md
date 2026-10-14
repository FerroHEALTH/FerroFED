- A registry in FHIR form or read from an mCSD directory declares a member's
  own exchange-format endpoint, as the TOML form does (#883). An `Endpoint`
  with `connectionType` `hl7-fhir-rest` in HL7's `endpoint-connection-type`
  names its categories as `payloadType` codings in HL7 Europe's priority
  category code system, its profiles in `payload-profile` extensions and the
  identifier system of the member's `ehr_id` in one `ehr-id-system`
  extension. Without them it is refused as before, under the rules the TOML
  form's native endpoint meets; the registry page of the book has the
  mapping. `ihe-iti` 0.0.34 reads `payloadType` codings and `valueCanonical`
  and `valueUri` extensions of a directory `Endpoint`.
