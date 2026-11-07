- A node's onward lane of kind `federation` carries another domestic
  party's organisation, a request with no named person, where its
  credentials section sets `onward_organisations = true`. The organisation
  reaches the node read-only: a write is refused `403` before any token is
  asked for. A lane with `correlation_claim` refuses a caller whose token
  does not carry the claim, an opaque token included, before the token
  endpoint is asked (#1710).
