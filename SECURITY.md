<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Security Policy

This document says how long each release is supported, how to report a
vulnerability privately, and what FerroFED's manufacturer reports to the
authorities. FerroFED's manufacturer is Cadasto B.V. It places each tagged
release on the market as one product: the source tag, the binary tarballs
and the gateway and console images of that version. The obligations come
from Regulation (EU) 2024/2847, the Cyber Resilience Act (CRA), and from
Regulation (EU) 2025/327 on the European Health Data Space (EHDS). This page
is not legal advice.

## Supported versions

Each release has a support period, during which Cadasto B.V. handles the
vulnerabilities that affect it as CRA Annex I Part II requires (Art 13(8)).

- **Length:** five years from the release date, the minimum Art 13(8),
  third subparagraph, sets. Each end date in the table below is computed
  from that rule. If Cadasto B.V. sets a longer period, this page states
  it.
- **Where fixes ship:** a security fix ships in a new release, never as a
  back-port into an older one, because a published release cannot be
  changed. Art 13(10) lets a manufacturer remediate vulnerabilities only in
  the version it placed on the market last, "provided that the users of the
  versions that were previously placed on the market have access to the
  version last placed on the market free of charge and do not incur
  additional costs to adjust the hardware and software environment". A user
  of an older release within its support period moves to the latest
  release to get the fix.
- **For whom that holds:** under [`LICENSE`](LICENSE), every release is
  free of charge for non-production use and for non-commercial production
  use. For a commercial licensee it depends on the terms of the commercial
  licence, and on whether the configuration changes a release's upgrade
  notes ask for count as costs to adjust the software environment. Both
  questions are with Cadasto B.V.'s counsel, and until they are answered
  this page does not claim that Art 13(10) holds for commercial licensees.
- **Updates stay available:** every security update stays available for at
  least 10 years after it is issued, or for the rest of the support period
  if that is longer (Art 13(9)). A published release, its tag and its image
  digest are never deleted ([`docs/release.md`](docs/release.md)).
- **After the end date:** a release past its end date is unsupported. The
  releases page keeps every version available, and running one past its
  end date leaves each vulnerability found after that date unfixed in it
  (Art 13(11)).

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
<!-- support periods: rows above this line -->

A withdrawn release gets no fix of its own either. It is listed under
[Withdrawn versions](#withdrawn-versions) below, and its advisory names the
release to move to.

## Reporting a vulnerability

**Do not open a public issue for a security vulnerability.** Report it
privately through GitHub's private vulnerability reporting:
<https://github.com/FerroHEALTH/FerroFED/security/advisories/new>. You will get
an acknowledgement within seven days. If that window passes with no response,
public disclosure to protect other users is your call.

If you have evidence that someone is exploiting the vulnerability, say so in
the report: an actively exploited vulnerability starts the 24-hour clock
below.

## Actively exploited vulnerabilities and severe incidents

CRA Art 14 applies from 11 September 2026 (Art 71(2)), including to every
release placed on the market before 11 December 2027 (Art 69(3)), so it
covers every FerroFED release.

- **What is reported:** an actively exploited vulnerability, one "for which
  there is reliable evidence that a malicious actor has exploited it in a
  system without permission of the system owner" (Art 3(42), Art 14(1)), and
  a severe incident having an impact on the security of FerroFED (Art 14(3),
  (5)).
- **Where:** through the single reporting platform ENISA runs (Art 16(1)),
  at the electronic notification end-point of the CSIRT designated as
  coordinator in the Member State of Cadasto B.V.'s main establishment, the
  Netherlands, and simultaneously to ENISA (Art 14(1), (3), (7)).
- **When:** an early warning within 24 hours of becoming aware, a
  notification within 72 hours, and a final report. For a vulnerability the
  final report is due no later than 14 days after a corrective or
  mitigating measure is available (Art 14(2)); for a severe incident, within
  one month after the incident notification (Art 14(4)).
- **Telling users:** Cadasto B.V. tells the impacted users, and where
  appropriate all users, of the vulnerability or incident and of the
  measures they can take (Art 14(8)). The channel is a
  [GitHub security advisory](https://github.com/FerroHEALTH/FerroFED/security/advisories)
  on this repository, with a direct message to every user Cadasto B.V.
  knows from a contract.

The procedure is on the book's
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