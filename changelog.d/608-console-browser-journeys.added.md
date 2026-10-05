- Browser journeys for the operator console (#608): headless Chrome, driven
  over WebDriver from Rust with `thirtyfour`, signs in at a test OpenID
  Provider, opens each operator view and follows its pagers, runs a query
  that shows every node's status and whether the answer is complete, sends a
  query the gateway refuses, and signs out, against a running gateway over
  stub nodes and the console serving its release site bundle. Each journey
  fails on any error the browser logs. They run behind
  `FERROFED_JOURNEYS=1` in the new `journeys (browser)` CI job, at the Chrome
  for Testing release `docs/VERSIONS.md` pins.
