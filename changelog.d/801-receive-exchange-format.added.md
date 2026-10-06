- `eehrxf` 0.0.6: the interoperability component's receive side (#801, part
  of #665). Under `fhir-r4`, `receive::ReceivedDocument::read` decodes a
  document received in the exchange format against FHIR R4, holds it to the
  R4 document rules (`bdl-9`, `bdl-10`, `bdl-11`), resolves its subject to
  its `Patient` entry and keeps its text byte for byte, and
  `ReceivedDocument::check` holds it to a `Bundle` and a `Composition`
  profile read from the vendored package: cardinalities, `fixed[x]` and
  `pattern[x]` values, and `value`, `pattern`, `exists` and `type` slices,
  with every constraint it cannot evaluate listed. Under `openehr`,
  `Mapping::to_openehr` maps the document into one canonical composition
  through FHIRconnect and keeps the document in the composition's
  `FEEDER_AUDIT.original_content` as a `DV_PARSABLE`. `dataset::Element`
  now carries its `fixed[x]` or `pattern[x]` value and its slicing
  (`dataset::constraint`). The gateway does not serve the receive path
  yet; writing a received document to a member is #802.
