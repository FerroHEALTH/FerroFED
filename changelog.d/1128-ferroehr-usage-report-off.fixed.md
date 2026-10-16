- The four FerroEHR nodes of the local `compose.yaml` stack, and every
  FerroEHR container the test harness starts, run with
  `FERROEHR__USAGE_REPORT__ENABLED=false` (#1128). A container does not
  inherit the CI runner's environment, so FerroEHR's own CI detection never
  fired, and each test run or local stack reported itself to FerroPULSE as an
  installation.
