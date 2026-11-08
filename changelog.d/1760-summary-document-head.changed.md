- A member's own patient summary is read for its `Bundle.type` and
  `Bundle.identifier` through the `fhir-types` value tree, and an element
  FHIR R4 does not define still never refuses it. A success that is no
  document `Bundle` with an identifier is now logged at `WARN`, naming the
  member's endpoint and what the answer misses, never a value it carries
  (#1760).
