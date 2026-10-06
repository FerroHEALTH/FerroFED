- The two queries on the container page post the query (#721). Each read
  `-d` followed by a number, which curl sent as the body, so the gateway
  refused it and the here-document went nowhere; they read `-d @-` now, as
  the README does. A guard in CI refuses a documentation `curl` command that
  sends a bare number as its body, or that takes a here-document with any
  `-d` other than `@-` or `@<file>`.
