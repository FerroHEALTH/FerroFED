- An onward lane takes `cache = false` in its
  `[credentials."<endpoint id>".onward]` table, so every call exchanges a
  token at the issuer. It is meant for a performance measurement's
  self-test, never for production; the cache stays on by default (#1620).
