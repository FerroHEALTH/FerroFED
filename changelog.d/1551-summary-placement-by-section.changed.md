- Moved to FerroBRIDGE's `eehrxf` 0.1.149 with `fhirconnect` 0.1.218,
  `openehr-mapping-core` 0.1.141 and `ferrobridge-validator` 0.1.144, on
  the `openehr-*` crates 0.0.128.
- The patient summary selects its medication entries by the template
  section that holds them, as `eehrxf`'s
  `crosswalk::patient_summary::SOURCES` places them: the medication summary
  takes a medication order under an ad hoc section, and the immunisations a
  medication action under the vaccination list section. A vaccine recorded
  as a medication action no longer reaches the medication summary, and a
  medication action under any other section reaches no section; record
  current medication as orders.
- A `[[fhir.mapping]]` takes an ADL 2 `template` with the `archetypes` it
  specialises, beside an OPT 1.4 `.opt`.
