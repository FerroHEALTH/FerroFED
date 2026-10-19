- Only the newest release is supported. `SECURITY.md`, the book and the
  release notes say that every earlier release stops receiving fixes when a
  new one is published, with no maintenance branches, no long-term-support
  line and no backports, and that upgrading is free. Cadasto B.V. commits to
  keep publishing security fixes for at least five years after each release
  (Regulation (EU) 2024/2847 Art 13(8)) and ships them only in the newest
  release (Art 13(10)); the end date each release states is the date until
  which that commitment runs. The startup banner's `Support` line reads
  `newest release only, fixes until YYYY-MM-DD`, the notice for a release
  past its end date says to upgrade to the newest release, and the console
  labels the date `Fixes until`. The `supported_until` member of
  `OPTIONS {base}/` keeps its name and value.
