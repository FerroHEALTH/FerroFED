- A `[[fhir.serve]]` member of `Electronic-Prescriptions` can be declared
  in the map mode, `{ member = "node-a", mode = "map", template = "…" }`,
  with its operational template: an OPT 1.4, or an ADL 2 template with the
  archetypes it specialises. The face then reads the member's medication
  orders over ITS-REST with `eu.ferrofed.eehrxf::medication-orders`, keeps
  the compositions the member's own system created, and maps each into an
  HL7 Europe MPD 1.0.0 `MedicationRequest-eu-mpd` through the MPD project
  of `eehrxf`, listed beside the native members' prescriptions and read by
  the id the search names. The start and `config check` refuse a template
  the project does not compile against.
