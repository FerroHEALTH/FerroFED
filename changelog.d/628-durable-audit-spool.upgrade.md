- If you run the Kubernetes example, move from `deployment.yaml` to
  `statefulset.yaml` (#628). Replace the `encrypted` storage class with
  your own. Wait until `GET /health/dependencies` reads `up` for the audit
  repository, so the spool on the old `emptyDir` is drained, then delete the
  Deployment and apply the StatefulSet.
