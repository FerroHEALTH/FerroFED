- A document received on the FHIR face whose codings carry no `display` is
  now refused with a `422` unless `[eehrxf.terminology]` names a terminology
  server to look the displays up (#687). Senders that omit displays need the
  server configured.
