- The PMIR identity feed across several replicas (#626). A draining gateway
  no longer deletes the ITI-94 subscription that the other replicas behind
  the same `[pmir] callback_url` rely on: the new `on_drain` key keeps it by
  default, for the other replicas and the next start, and `on_drain =
  "unsubscribe"` deletes it, for the last gateway or a replica with a
  `callback_url` of its own. Replicas that find several subscriptions for
  their `callback_url` adopt the same one and delete the others, and a
  delete the Registry answers `404` or `410` counts as done. The book's
  identity page describes the two ways to run the feed across replicas and
  the window during which a replica the change did not reach keeps a stale
  binding.
