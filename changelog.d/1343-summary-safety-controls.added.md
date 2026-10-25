- The patient summary carries the safety controls of the review of 2026-10-25
  (#1327): the medication summary marks two members' entries of one medicine,
  strength and route as a probable duplicate in its narrative, naming both
  members and merging nothing (#1343); the medical alert and the functional
  status are always written, as `unavailable`, with a narrative saying the
  gateway does not supply them (#1344); the medication summary also selects
  `openEHR-EHR-INSTRUCTION.medication_order.v3`, and a mapping whose context
  does not apply to a composition leaves it to the section's other mappings
  (#1345); every element a mapping run dropped is named in its section's
  narrative by path and member (#1350).
