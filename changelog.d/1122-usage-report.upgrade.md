- The release's compose file and the Kubernetes example now set
  `FERROFED__USAGE_REPORT__STATE_DIR`, so a gateway started from them sends
  the installation report to FerroPULSE. Set
  `FERROFED__USAGE_REPORT__ENABLED=false` before you upgrade to keep it off
  (#1122).
