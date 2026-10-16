- `GET {base}/operator/deployment`, on the operator surface behind the
  operator scope, reports the deployment profile the gateway started with
  (`production` or `development`), whether patients resolve through the
  `[dev]` cross-reference, the product version and the replica name, so
  the operator console can warn that it reads a development gateway. It
  carries no setting value, URL, credential or patient identifier.
