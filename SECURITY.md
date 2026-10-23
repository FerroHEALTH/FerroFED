<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Security Policy

This file holds what is particular to FerroFED: which release is supported,
until when security fixes are published, where to report a vulnerability in
FerroFED, and the versions withdrawn. The vulnerability-handling policy of
Cadasto B.V., the manufacturer of every FerroHEALTH product, is on the
family book's [Security](https://ferrohealth.eu/docs/security.html) page:
what happens after a report, how a fix and its advisory reach you, how to
hear of an update, and what the manufacturer reports to the authorities.
This page is not legal advice.

One FerroFED release is the source tag, the binary tarballs and the gateway
and console images of one version. Cadasto B.V. places each tagged release on
the market as one product under Regulation (EU) 2024/2847, the Cyber
Resilience Act (CRA), and as an EHR system under Regulation (EU) 2025/327 on
the European Health Data Space (EHDS)
([regulatory status](https://ferrofed.eu/docs/evaluate/regulatory-status.html)).

## Supported versions

Only the newest release is supported. When a new release is published, every
earlier release stops receiving fixes: there are no maintenance branches, no
long-term-support line and no backports. If you are not on the newest
release, upgrade; upgrading is free. Under the Cyber Resilience Act, Cadasto
B.V. commits to keep publishing security fixes for at least five years after
each release (Art 13(8)) and ships them only in the newest release
(Art 13(10)).

| Artifact | Supported version | Where fixes ship |
| --- | --- | --- |
| The `ferrofed` binary (release tarballs) | the newest published release | the newest release |
| The gateway and console container images | the newest published release | the newest release |

- **The five years:** Cadasto B.V. keeps publishing security fixes, in new
  releases, for at least five years after each release is placed on the
  market, the minimum Art 13(8), third subparagraph, sets. Each end date in
  the table below is computed from that rule: it is the date until which
  Cadasto B.V. commits to keep publishing fixed releases, and a release
  itself is supported only while it is the newest. If Cadasto B.V. sets a
  longer period, this page states it. The matters Art 13(8) names, and how
  each was taken into account, are recorded in the book's
  [basis of the five years](https://ferrofed.eu/docs/evaluate/regulatory-status.html#the-basis-of-the-five-years)
  (Annex VII, point 4).
- **Where fixes ship:** a security fix ships in a new release. Where it
  is technically feasible, that release is a patch with the fix alone, so
  the update is provided separately from functionality updates (Annex I
  Part II(2)); when a functional change has to ship with it, the
  release's `### Security` section opens with "Shipped with functional
  changes because:" and the reason, and the release notes carry it.
  Art 13(10) lets Cadasto B.V. ship it only in the newest release,
  provided a user of an earlier release gets the newest one free of charge
  and with no added cost to adjust the environment it runs in. Every
  release is published under the same licence terms as the one before it,
  and a published release is never changed, so a user of an older release
  upgrades to the newest release to get the fix.
- **Updates stay available:** a published release, its tag and its image
  digest are never deleted ([`docs/release.md`](docs/release.md)).
- **An older release:** a release that is not the newest receives no fix.
  The releases page keeps every version available, and running an older
  one leaves each vulnerability fixed in a later release unfixed in it
  (Art 13(11)). From v0.0.10 on, the gateway knows its own end date from
  its build, names it in the startup banner and in `OPTIONS {base}/`, and
  once the date has passed says so in the banner, at start in its log and
  in `ferrofed config check` (Art 13(19)).

Art 13(8) binds the releases placed on the market from 11 December 2027
(Art 69(2), Art 71(2)). Cadasto B.V. gives the releases placed before that
date the same commitment. A pre-release (a tag with a suffix, such as
`v0.0.2-rc.1`) rehearses the release lane and has no support period. `main`
is development code that is not supplied for use.

From v0.0.10 on, the release notes of each release open with its end date,
and `scripts/release/changelog.sh --assemble` adds its row here when the
release is cut ([`docs/release.md`](docs/release.md#before-the-tag)). The
notes of an earlier release cannot be changed, so its end date is stated
here and in the copy of this table on the book's
[CRA information to the user](https://ferrofed.eu/docs/evaluate/cra-user-information.html#7-technical-security-support-and-the-support-period),
which `scripts/checks/support-periods.sh` holds to this one.

| Release | Released | Fixes published until |
| ------- | -------- | --------------------- |
| v0.0.1 | 2026-10-01 | 2031-10-01 |
| v0.0.3 | 2026-10-02 | 2031-10-02 |
| v0.0.6 | 2026-10-03 | 2031-10-03 |
| v0.0.7 | 2026-10-03 | 2031-10-03 |
| v0.0.8 | 2026-10-04 | 2031-10-04 |
| v0.0.9 | 2026-10-05 | 2031-10-05 |
| v0.0.10 | 2026-10-15 | 2031-10-15 |
| v0.0.11 | 2026-10-16 | 2031-10-16 |
<!-- support periods: rows above this line -->

A withdrawn release gets no fix of its own either. It is listed under
[Withdrawn versions](#withdrawn-versions) below, and its advisory names the
release to move to.

## Reporting a vulnerability

**Do not open a public issue for a security vulnerability.** This route
covers what this repository publishes: the `ferrofed` binary, the gateway
and console images (`ferrofed`, `ferrofed-console`) and the library crates.
Report privately, through either route:

- [GitHub private vulnerability reporting](https://github.com/FerroHEALTH/FerroFED/security/advisories/new)
  ("Report a vulnerability" on the repository's Security tab), which needs a
  GitHub account; or
- email to [info@cadasto.com](mailto:info@cadasto.com), the single point of
  contact of Cadasto B.V., the manufacturer (CRA Art 13(17)), with "FerroFED
  vulnerability" in the subject. No account is needed. The address has no
  published encryption key, so send the details you are comfortable sending
  by email and say that more is available; a channel for the rest is agreed
  with you.

Both routes reach the same people and the same procedure. You will get an
acknowledgement within seven days. If that window passes with no response,
public disclosure to protect other users is your call.

A patient identifier reaching a node is a vulnerability in FerroFED: the
gateway's central safety property is that no request it composes for a node
carries one. Report it here, privately.

If you have evidence that someone is exploiting the vulnerability, say so in
the report: an actively exploited vulnerability starts the 24-hour clock
below.

## Security advisories

A reported vulnerability gets a private draft of a GitHub security advisory
on this repository the day it is confirmed, and the draft is the record
until the fix ships. Every fixed vulnerability is published as that
advisory, in the same hour as the release that fixes it: one in FerroFED's
own code, and one for a dependency or base-image fix that changes what a
shipped binary or image contains. The `### Security` entry for the fix in
[`CHANGELOG.md`](CHANGELOG.md) carries the advisory's `GHSA-` identifier,
and the release notes repeat it (CRA Annex I Part II(4), (8)). A dependency
finding that an OpenVEX statement under `security/vex/` shows does not
affect FerroFED gets no advisory; the changelog entry names that statement
instead. The advisories are machine-readable through the GitHub REST API
(`GET /repos/FerroHEALTH/FerroFED/security-advisories`). The procedure is
in [`docs/post-market.md`](docs/post-market.md#a-fixed-vulnerabilitys-advisory).

## Actively exploited vulnerabilities and severe incidents

CRA Art 14 covers every FerroFED release. Cadasto B.V. notifies an actively
exploited vulnerability or a severe incident through ENISA's single
reporting platform, with an early warning within 24 hours, as the family
book's
[Security](https://ferrohealth.eu/docs/security.html#what-cadasto-bv-reports-to-the-authorities)
page describes. FerroFED tells its users through a
[GitHub security advisory](https://github.com/FerroHEALTH/FerroFED/security/advisories)
on this repository, with a direct message to every user Cadasto B.V. knows
from a contract. FerroFED's own procedure is on the book's
[Complaints and incidents](https://ferrofed.eu/docs/evaluate/post-market.html#actively-exploited-vulnerabilities-and-severe-incidents)
page and in [`docs/post-market.md`](docs/post-market.md).

## Serious incidents and non-conforming versions

The EHDS adds duties of its own. A vulnerability that harmed a person, or
could, may be a serious incident, which the manufacturer reports to the
market surveillance authorities within three days of becoming aware of it
(EHDS Art 44(7)). This report is separate from the CRA notifications above:
one event can need both, and neither stands in for the other. Report it as
above, and also write to
[info@cadasto.com](mailto:info@cadasto.com) with "FerroFED incident" in the
subject. A vulnerability that made a version non-conforming is entered in
the
[register of non-conforming versions](docs/registers/non-conforming-versions.tsv)
once its advisory is published. The procedures are on the book's
[Complaints and incidents](https://ferrofed.eu/docs/evaluate/post-market.html)
page and in [`docs/post-market.md`](docs/post-market.md).

## Withdrawn versions

A version Cadasto B.V. found not to conform and withdrew (Regulation (EU)
2025/327 Art 30(1)(i)) stays listed here. It is not supported, and its
advisory says which version to move to. A published release cannot be
changed or deleted, so the release, its tag and its image digest remain;
the `<major>.<minor>` and `latest` image tags move to the replacement.
`scripts/release/withdraw.sh` adds the row, as
[`docs/release.md`](docs/release.md) describes.

| Version | Withdrawn | Finding | Advisory |
| ------- | --------- | ------- | -------- |
<!-- withdrawn versions: rows above this line -->

## Vulnerability exploitability (VEX)

A scanner can report a finding in a FerroFED artifact that does not affect
it, such as an unmaintained crate that only runs inside the compiler. Such
findings are judged in OpenVEX documents under
[`security/vex/`](security/vex/README.md), which a scanner reads with
`trivy --vex`. Each release attaches its vulnerability record,
`ferrofed-vX.Y.Z-vulnerability-record.json`: every finding of a full scan of
both images and the binaries, each with the statement that judges it, and
the release is refused while a finding has none.
