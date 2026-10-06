- The gateway knows the end of its release's support period from its build,
  never from the network (Regulation (EU) 2024/2847 Art 13(19)). The startup
  banner names it on a `Support` line, and `OPTIONS {base}/` declares it as
  the top-level `supported_until`, or `null` for a build from source or a
  pre-release. Once the date has passed, the banner shows an `UNSUPPORTED`
  notice, `serve` logs it at `WARN` and `ferrofed config check` prints it as
  a warning. `scripts/checks/versions.sh` fails when a release in
  `CHANGELOG.md` has no row in the `SECURITY.md` support table or a row
  whose end date is not five years after its release date.
