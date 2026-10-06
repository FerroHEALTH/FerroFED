- The two federated-query examples on the book's container page post the
  query from the here-document with `-d @-`; they posted a number, which the
  gateway refused. A guard, `scripts/checks/curl-examples.sh`, refuses a
  `curl` example that opens a here-document and passes `-d` anything but
  `@-` or `@<file>` (#721).
