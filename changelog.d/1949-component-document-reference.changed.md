- Each report the gateway writes is listed with the interoperability
  component's own description of it, which FerroBRIDGE's validator holds to
  both HL7 Europe `EehrxfMhdDocumentReference` and MyHealth@EU
  `DocumentReference-minimal`: its version, language and period come from
  the member's composition, its one `category` holds the document category
  beside the priority category, and its master identifier is marked
  `usual`. A written discharge report is now typed `34105-7` and a written
  imaging report `18748-4`, the codes `DocumentReference-minimal` requires,
  so a search for `18842-5` or `85430-7` no longer returns them. FerroFED
  moves to `eehrxf` 0.40, `ferrobridge-term` 0.21, `ferrobridge-validator`
  0.22 and `ferrohealth-smart` 0.35, whose own JWS library now verifies the
  gateway's signed document URLs (#1949).
