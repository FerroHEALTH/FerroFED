<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Security Policy

This file holds what is particular to FerroFED: which releases are supported
and until when, where to report a vulnerability in FerroFED, and the
versions withdrawn. The vulnerability-handling policy of Cadasto B.V., the
manufacturer of every FerroHEALTH product, is on the family book's
[Security](https://ferrohealth.eu/docs/security.html) page: what happens
after a report, how a fix and its advisory reach you, how to hear of an
update, and what the manufacturer reports to the authorities. This page is
not legal advice.

One FerroFED release is the source tag, the binary tarballs and the gateway
and console images of one version. Cadasto B.V. places each tagged release on
the market as one product under Regulation (EU) 2024/2847, the Cyber
Resilience Act (CRA), and as an EHR system under Regulation (EU) 2025/327 on
the European Health Data Space (EHDS)
([regulatory status](https://ferrofed.eu/docs/evaluate/regulatory-status.html)).

## Supported versions

Each release has a support period, during which Cadasto B.V. handles the
vulnerabilities that affect it as CRA Annex I Part II requires (Art 13(8)).

- **Length:** five years from the release date, the minimum Art 13(8),
  third subparagraph, sets. Each end date in the table below is computed
  from that rule. If Cadasto B.V. sets a longer period, this page states
  it.
- **Where fixes ship:** a security fix ships in a new release, never as a
  back-port into an older one, so a user of an older release within its
  support period moves to the latest release to get the fix. The family
  book says why, and for whom CRA Art 13(10) allows it
  ([how a fix reaches you](https://ferrohealth.eu/docs/security.html#how-a-fix-reaches-you)).
- **Updates stay available:** a published release, its tag and its image
  digest are never deleted ([`docs/release.md`](docs/release.md)).
- **After the end date:** a release past its end date is unsupported. The
  releases page keeps every version available, and running one past its
  end date leaves each vulnerability found after that date unfixed in it
  (Art 13(11)). From v0.0.10 on, the gateway knows its own end date from
  its build, names it in the startup banner and in `OPTIONS {base}/`, and
  once the date has passed says so in the banner, at start in its log and
  in `ferrofed config check` (Art 13(19)).

Art 13(8) binds the releases placed on the market from 11 December 2027
(Art 69(2), Art 71(2)). Cadasto B.V. gives the releases placed before that
date the same support period. A pre-release (a tag with a suffix, such as
`v0.0.2-rc.1`) rehearses the release lane and has no support period. `main`
is development code that is not supplied for use.

From v0.0.10 on, the release notes of each release open with its end date,
and `scripts/release/changelog.sh --assemble` adds its row here when the
release is cut ([`docs/release.md`](docs/release.md#before-the-tag)). The
notes of an earlier release cannot be changed, so its end date is stated
here alone.

| Release | Released | Supported until |
| ------- | -------- | --------------- |
| v0.0.1 | 2026-10-01 | 2031-10-01 |
| v0.0.3 | 2026-10-02 | 2031-10-02 |
| v0.0.6 | 2026-10-03 | 2031-10-03 |
| v0.0.7 | 2026-10-03 | 2031-10-03 |
| v0.0.8 | 2026-10-04 | 2031-10-04 |
| v0.0.9 | 2026-10-05 | 2031-10-05 |
| v0.0.10 | 2026-10-15 | 2031-10-15 |
<!-- support periods: rows above this line -->

A withdrawn release gets no fix of its own either. It is listed under
[Withdrawn versions](#withdrawn-versions) below, and its advisory names the
release to move to.

## Reporting a vulnerability

**Do not open a public issue for a security vulnerability.** Report it
privately through GitHub's private vulnerability reporting:
<https://github.com/FerroHEALTH/FerroFED/security/advisories/new>. You will get
an acknowledgement within seven days. If that window passes with no response,
public disclosure to protect other users is your call. Without a GitHub
account, use the manufacturer's email route on the family book's
[Security](https://ferrohealth.eu/docs/security.html#reporting-a-vulnerability)
page.

A patient identifier reaching a node is a vulnerability in FerroFED: the
gateway's central safety property is that no request it composes for a node
carries one. Report it here, privately.

If you have evidence that someone is exploiting the vulnerability, say so in
the report: an actively exploited vulnerability starts the 24-hour clock
below.

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
