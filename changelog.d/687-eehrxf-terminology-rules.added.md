- The interoperability component carries the terminology rules of the
  exchange format (#687): a code in a preferred system passes as recorded,
  an openEHR `TERM_MAPPING` of match `=` adds a coding after the original, a
  broader, narrower or unknown mapping adds no coding and is written into
  the narrative, and a code in no preferred system follows Art 10(7) of the
  eHealth Network Guidelines on Patient Summary.
- An optional FHIR terminology server, `[eehrxf.terminology]`, is asked for
  `$translate` (only an `equivalent` or `equal` match becomes a coding),
  `$lookup` and `$validate-code`. A refusal or a time-out is an error that
  names the operation and the server's status.
- A received document is held to the `required` bindings of its `Bundle`
  and `Composition` profiles, against the value set expansions its package
  carries; a binding no expansion and no terminology server decides is
  listed as not evaluated. A received coding with no display is given one by
  `$lookup`, and refused when no terminology server is configured.
