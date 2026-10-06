- On a stop, the bindings' drain, such as deleting the PMIR subscription,
  runs under its own budget, `server.bindings_drain_timeout_ms` (5 seconds
  by default), after the requests in flight have drained; a binding still
  stopping when it elapses is abandoned with a warning (#676). The shipped
  grace periods are derived from the drain delay, the drain and this budget,
  and the health page shows the arithmetic.
- `scripts/checks/versions.sh` fails when a file the release ships or builds
  its images from is missing, where it skipped the check, and
  `scripts/checks/kubernetes-example.sh` and
  `scripts/checks/release-compose.sh` read every timeout of their grace
  period check from the example's configuration, where they assumed the
  gateway's default request timeout (#676).
