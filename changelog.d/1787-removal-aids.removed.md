- The named refusals of keys v0.0.16 and earlier releases removed or
  renamed are deleted, under the one-release rule for removal aids; each
  such key is now refused as an unknown key. They are `[dev]` (removed in
  v0.0.14) and the `[server]` keys v0.0.16 renamed (`server.public_url`,
  `server.replica`, `server.bindings_drain_timeout_ms` and
  `server.overload_retry_after_s`). The Upgrade notes of the release that
  removed each key give its step. The refusals of the keys v0.0.17 removed
  stay for this release, since v0.0.17 was never released (#1787).
