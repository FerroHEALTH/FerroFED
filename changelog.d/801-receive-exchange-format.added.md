- `eehrxf` 0.0.6: the interoperability component's receive side (#801, part
  of #665). Under `fhir-r4`, `receive::ReceivedDocument::read` reads a
  document received in the exchange format once: it refuses an object that
  repeats a name, decodes the text against FHIR R4, refuses a resource of a
  type R4 does not define and a document that does not encode back to what
  was read, holds it to the R4 document rules (`bdl-7` to `bdl-11`, and
  `fullUrl`s that agree with their resource), and resolves its subject to
  its one `Patient` entry. Every reference, found by its R4 type at any
  depth, is held to that patient: a `subject`, `patient`, `beneficiary` or
  `for` must name it, and a reference that could name another patient, a
  `type` that disagrees with its target, a second or contained `Patient` and
  an element the R4 table does not describe are refused. The
  text is kept byte for byte. `ReceivedDocument::check` holds the decoded
  document to a `Bundle` and a `Composition` profile read from the vendored
  package: cardinalities, `fixed[x]` and `pattern[x]` values, and `value`,
  `pattern`, `exists` and `type` slices, an entry of a type no slice names,
  and the invariants that require a reference on listed entry types, with
  every constraint it cannot evaluate listed. Under `openehr`, `Mapping::to_openehr` maps the document
  into one canonical composition through FHIRconnect and keeps the document
  in the composition's `FEEDER_AUDIT.original_content` as a `DV_PARSABLE`.
  `dataset::Element` now carries its `fixed[x]` or `pattern[x]` value and
  its slicing (`dataset::constraint`). The gateway does not serve the
  receive path yet; writing a received document to a member is #802.
