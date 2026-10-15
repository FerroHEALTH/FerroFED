- The access record of a write names the version it left at the member
  (Regulation (EU) 2025/327 Annex II 3.2): a routed create, update or delete
  records the `OBJECT_VERSION_ID` the node's `ETag` or `Location` names as
  the origin's `written-version`, and the `version-uid` of the composition
  it wrote; an answer that names no version, or no `OBJECT_VERSION_ID`,
  records `written-version-unread` as `unnamed` or `malformed` and names no
  value. A received document's record names its version the same way. The
  origin `status` of a routed read or write, and of a received document, is
  now the Federation Tier §11.1 status (`active`, `node-error`, `time-out`,
  `offline`, `consent-denied`) a query's origins already carry, in place of
  the HTTP status or `failed` (#946).
