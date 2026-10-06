- A federated query still waiting for a slot of a member's in-flight cap
  when the overall budget runs out is reported as capped with nothing sent,
  never as a member abandoned without an answer, so it no longer counts as a
  sent time-out in the member metrics (§11.1, §11.5, N38). The request log
  writes the closed method label in its `method` field, `_OTHER` for a
  method no RFC the gateway knows defines, so a client's method token no
  longer reaches the log (#708).
