- The Kubernetes example runs the gateway as a StatefulSet,
  `deploy/kubernetes/statefulset.yaml` in place of `deployment.yaml`, so
  each replica keeps its audit spool on a persistent volume claim of its own
  instead of an `emptyDir` a reschedule deletes (#628). The claim names an
  `encrypted` storage class to replace with your own, and is kept when the
  StatefulSet is deleted or scaled down. The container page says what a
  lost spool means for the audit trail. To move an existing Deployment,
  wait until `GET /health/dependencies` reads `up` for the audit
  repository, then delete the Deployment and apply the StatefulSet.
