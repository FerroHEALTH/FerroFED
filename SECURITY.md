<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Security Policy

This document says which versions receive security fixes and how to report a
vulnerability privately.

## Supported versions

Security fixes apply to the latest released version only; there is no back-port
line. `main` is best effort. Once the project reaches 1.0 this table will name a
supported minor line.

| Version             | Supported          |
| ------------------- | ------------------ |
| Latest release      | :white_check_mark: |
| Older releases      | :x:                |
| Withdrawn releases  | :x:                |
| `main` (unreleased) | best effort        |

A withdrawn release is unsupported whatever its age, and is listed under
[Withdrawn versions](#withdrawn-versions) below.

## Reporting a vulnerability

**Do not open a public issue for a security vulnerability.** Report it
privately through GitHub's private vulnerability reporting:
<https://github.com/FerroHEALTH/FerroFED/security/advisories/new>. You will get
an acknowledgement within seven days. If that window passes with no response,
public disclosure to protect other users is your call.

## Serious incidents and non-conforming versions

FerroFED's manufacturer, Cadasto B.V., has duties under Regulation (EU)
2025/327 beyond fixing a vulnerability. A vulnerability that harmed a person,
or could, may be a serious incident, which the manufacturer reports to the
market surveillance authorities within three days of becoming aware of it
(Art 44(7)). Report it as above, and also write to
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
