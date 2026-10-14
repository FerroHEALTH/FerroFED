- A member's own exchange-format endpoint, the `native` mode (#686): a
  second `[[endpoint]]` of a node with `connection_type = "hl7-fhir-rest"`,
  its `categories`, `profiles` and the `ehr_id_system` the member's
  `ehr_id` is written in. For a patient summary, the gateway leaves such a
  member out of its section queries and asks its endpoint
  `Patient/$summary` by `identifier=<ehr_id_system>|<ehr_id>` and
  `profile`, through the outbound identifier-hygiene gate, with the onward
  credential and the signed caller identity of the member's Query API
  endpoint, so no patient identifier reaches it (N33). The member's
  document comes back byte for byte, and every section of the gateway's
  document names the member as serving its own summary. A member whose
  endpoint does not answer makes the summary incomplete, `424` or `504`
  under all-or-nothing, and is named in every section under `partial`. The
  endpoint is absent from `OPTIONS {base}/`, and a directive or a targeting
  header naming it is `400`. `config check` refuses a native endpoint with
  no `ehr_id_system`, no category or no profile, a category code that is no
  Art 14(1) priority category, and a patient summary endpoint that does not
  declare the EPS composition profile.
