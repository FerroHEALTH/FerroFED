- The FHIR face reads a member's prescription search page through the
  `fhir-types` value tree. A page whose `entry`, `link`, `fullUrl`,
  `search.mode`, resource or identifier holds a JSON value of another kind
  than its FHIR JSON form is still unusable, and the reason now names that
  element, where it said the answer was no FHIR resource in JSON (#1755).
