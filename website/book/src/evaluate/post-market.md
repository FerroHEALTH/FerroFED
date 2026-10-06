<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Complaints and incidents

This page says how to complain about FerroFED, what its manufacturer records,
and what it does when a released version turns out not to conform or is
involved in a serious incident. It describes the manufacturer's procedures
under Regulation (EU) 2025/327 on the European Health Data Space (the EHDS
Regulation), Art 30(1)(i) to (o), Art 30(5) and Art 44. The
[regulatory status](regulatory-status.md) page sets out why those articles
reach FerroFED and from which date. The maintainers' working checklist for
the same procedures is
[`docs/post-market.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/post-market.md).

Every quotation is from the Official Journal text, vendored at
[`docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml).
This page is not legal advice.

## The manufacturer

FerroFED's manufacturer is Cadasto B.V., the Licensor named in
[`LICENSE`](https://github.com/FerroHEALTH/FerroFED/blob/main/LICENSE):

| | |
|---|---|
| Name | Cadasto B.V. |
| Postal address | Comeniusstraat 2d, 1817 MS Alkmaar, The Netherlands |
| Single point of contact | [info@cadasto.com](mailto:info@cadasto.com) |
| Website | <https://www.cadasto.com/contact/> |

Art 30(1)(g) asks for these details "in the EHR system", with "a single point
at which the manufacturer can be contacted". The running gateway shows them
in `GET {base}/`, in `OPTIONS {base}/` under `manufacturer`, in its startup
banner and in `ferrofed --version`; the operator console shows them in the
footer of every page, and both container images carry them as labels.

## Making a complaint

Art 30(1)(n) asks the manufacturer to "establish channels of complaint". A
complaint is any report that FerroFED does not do what its documentation or
its intended purpose says, or that it falls short of the Regulation. Pick the
channel by what the report contains:

- **A vulnerability, or a patient identifier reaching a node:** report it
  privately through
  [GitHub private vulnerability reporting](https://github.com/FerroHEALTH/FerroFED/security/advisories/new),
  as [`SECURITY.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/SECURITY.md)
  says.
- **Anything that may have harmed a person, or could:** write to
  [info@cadasto.com](mailto:info@cadasto.com) with "FerroFED incident" in the
  subject, so it is handled as a possible serious incident from the first
  hour (see [Serious incidents](#serious-incidents)).
- **Any other complaint:** write to [info@cadasto.com](mailto:info@cadasto.com)
  with "FerroFED complaint" in the subject, or open a
  [GitHub issue](https://github.com/FerroHEALTH/FerroFED/issues/new/choose)
  when the report can be public.

Attach the archive `ferrofed report` writes (see
[The report archive](#the-report-archive)), and say how FerroFED is deployed
and what happened. Where the archive cannot be made, name the version you run
(`ferrofed --version` prints it). Never send patient data: describe the case,
or build a synthetic one.

Each distributor, importer and authorised representative of FerroFED is told
of these channels and of the registers below (Art 30(1)(n), (o)).

## The registers

Art 30(1)(o) asks the manufacturer to "keep a register of complaints and a
register of non-conforming EHR systems". Both are kept in the repository, so
anyone can read them:

- [`docs/registers/complaints.tsv`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/registers/complaints.tsv):
  every complaint, whatever the channel, with the versions it concerns, how
  it was classified, the record of the work on it and its outcome.
- [`docs/registers/non-conforming-versions.tsv`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/registers/non-conforming-versions.tsv):
  every released version found not to conform, with the requirement it
  missed, the corrective action, and when the authorities and the users were
  told.

No row names the person who complained or carries patient data. The
manufacturer keeps the correspondence itself, outside the repository. A
vulnerability enters the registers once its advisory is published, so a row
never discloses one before its fix.

## Corrective action, recall and withdrawal

When the manufacturer considers, or has reason to believe, that a released
version is not, or is no longer, in conformity with "the essential
requirements laid down in Annex II", it takes "without undue delay any
necessary corrective action", or recalls or withdraws the version (Art
30(1)(i)). The steps:

1. The version enters the register of non-conforming versions, with the
   Annex II item or the obligation it misses.
2. The fix is an issue on the tracker and ships in a new patch release. A
   published release is never changed or deleted, so a non-conforming version
   is named in the register, in the security advisory where there is one,
   and in the release notes of the version that corrects it.
   A withdrawn version is also listed as unsupported in
   [`SECURITY.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/SECURITY.md#withdrawn-versions),
   and its `<major>.<minor>` and `latest` image tags move to the correcting
   release. Its own version tag and image digest stay, so a deployment pinned
   to either keeps running it until the deployment moves.
3. The national authorities of each Member State where the version was made
   available or put into service are told of the non-conformity, the
   corrective action "including the timetable for implementation", and the
   date the version was "brought into conformity or been recalled or
   withdrawn" (Art 30(1)(i)).
4. Distributors, the authorised representative, importers and users are told
   of the non-conformity and of the corrective action, recall or withdrawal
   (Art 30(1)(j)), through the advisory or the release notes, and directly
   where the manufacturer knows them.

The same channels carry any "mandatory preventive maintenance" and its
frequency (Art 30(1)(k)). Where an authority finds that FerroFED "has caused
harm to the health or safety of natural persons", the manufacturer "shall
immediately provide information and documentation" to the affected person or
user (Art 44(3)), and the corrective action covers every copy placed on the
market in the Union (Art 44(4)).

## Serious incidents

Art 2(2)(r) defines a serious incident as "any malfunction or deterioration
in the characteristics or performance of an EHR system made available on the
market that directly or indirectly leads, might have led or might lead to any
of the following: (i) the death of a natural person or serious harm to a
natural person's health; (ii) serious prejudice to a natural person's rights;
(iii) serious disruption of the management and operation of critical
infrastructure in the health sector".

The manufacturer handles these as possible serious incidents of a
federation gateway: an answer presented as complete while a member that
holds the patient's data was left out, a follow-up read or write routed to
the wrong record, and a patient identifier reaching a node.

Art 44(7) sets the report:

- **To whom:** "the market surveillance authorities of the Member States
  where such serious incident occurred and of the Member States where such
  EHR systems are placed on the market or put into service". Each Member
  State designates its authority, and "the Commission and the Member States
  shall make that information publicly available" (Art 43(2)).
- **What:** the incident, with "a description of the corrective action taken
  or envisaged by the manufacturer".
- **When:** "immediately after the manufacturer has established a causal link
  between the EHR system and the serious incident or the reasonable
  likelihood of such a link and, in any event, not later than three days
  after the manufacturer becomes aware of the serious incident". The three
  days run from awareness, so a report goes in on time even while the cause
  is still being established, and is completed later.

**NIS 2.** The report is made "without prejudice to incident notification
requirements under Directive (EU) 2022/2555" (Art 44(7)). The two duties are
separate: a report to a market surveillance authority does not stand in for
a notification NIS 2 requires, and a NIS 2 notification does not stand in for
this report. Which entities NIS 2 binds, and its own deadlines, are set by
that Directive, which is not vendored here; a deployment's counsel answers
them for that deployment.

**Personal data.** Where a serious incident concerns personal data
protection, the market surveillance authority informs the data protection
supervisory authorities (Art 44(6)). A deployment's own duties under data
protection law are its own.

## The report archive

A serious-incident report is due within three days of awareness (Art
44(7)), and an authority that acts on one passes on "the data necessary for
the identification of the EHR system concerned" (Art 44(5)). Run
`ferrofed report` on the host the gateway runs on, with the same
configuration, to collect that data in one step:

```sh
ferrofed report --config /etc/ferrofed/ferrofed.toml \
  --operator-token-file /run/secrets/ferrofed/operator-token \
  --out ferrofed-report.tar
```

The command writes an uncompressed tar archive. Without `--out`, it is
`ferrofed-report-<UTC time>.tar` in the working directory. The command never
overwrites a file. Every file sits under `ferrofed-report/`:

| File | What it holds |
|---|---|
| `manifest.json` | `format` (`ferrofed-report`), `format_version` (`1`), the time it was made, the product, version and manufacturer, an `entries` list with each file's purpose, its source, its size and its SHA-256, and a `missing` list with each part that could not be read and why |
| `build.json` | the version, the commit and target triple the release build recorded, the platform, whether it is a release build, the Cargo features (the bindings), and the pinned specification and `openehr-*` releases the startup banner prints |
| `release.json` | the release tag, the tarball and image of this version, the workflows that signed their attestations, and the `gh attestation verify` commands that check them |
| `configuration.toml` | the effective configuration (the file with every `FERROFED__` override applied), redacted |
| `health/readiness.json` | `GET {base}/health/readiness`, as the gateway answered it |
| `health/dependencies.json` | `GET {base}/health/dependencies` |
| `incidents.json` | `GET {base}/operator/incidents`: every integrity incident kind's count and the most recent of each, read with the operator token |
| `metrics.txt` | `GET /metrics` from the admin listener, when `metrics.listen` is set |

The live files come from the gateway running on this host. When it is
stopped, or a part needs something the deployment did not give (the operator
token, an admin listener), the part is listed under `missing` with the reason,
and the command prints the same list. The image digest is not visible from
inside the process. For a container, add the output of
`docker image inspect --format '{{index .RepoDigests 0}}' <image>`.

Redaction is deny by default and runs over the parsed configuration, so a
comment or an inline table cannot pass it. A key stays when it is a field
name, so the shape is visible. The keys of a table keyed by data
(`credentials`, `members`, `namespaces`, `communities`) and any key that is
not a `snake_case` field name become `***1`, `***2` and so on. A value stays
only when it is one of these:

- a number, a boolean or a date;
- the scheme, host and port of a URL, with its userinfo, path, query and
  fragment replaced by `***`;
- a closed setting or a listen address, such as `profile`, `listen`,
  `base_path`, `format` or `node_selection`.

Every other value is `***`. That covers file paths, which can name a
person's home directory, namespaces, user names, and every value under
`[dev]`, whose cross-reference pairs patient identifiers with `ehr_id`s.

The live files keep only the fields that carry states, counts, kinds, times
and routing ids:

- Readiness keeps each indicator's state without its detail text.
- The incidents keep their kind, time, detection and the endpoints and
  nodes involved, without the description, the `ehr_id` or the
  `creating_system_id`.
- The metrics keep the value of a label only when the gateway itself sets
  that label.
- A part that cannot be read is named in `missing` without quoting what was
  answered.

The archive therefore carries no patient identifier, `ehr_id`, clinical
payload, credential, personal name, header value or URL userinfo. Read it
before you send it all the same. A secret or a patient identifier found in
an archive is a vulnerability: report it as one.

## Cooperation with the authorities

On request, the manufacturer gives a market surveillance authority "all the
information and documentation necessary to demonstrate the conformity" of
FerroFED, in an official language of the Member State concerned (Art
30(1)(l)), "in paper or electronic form" and "in a language which can be
easily understood by that market surveillance authority" (Art 30(5)), and
cooperates on any action to bring FerroFED into conformity or to eliminate
its risks (Art 30(1)(m), Art 44(1)). A request goes to the single point of
contact above. An authority may restrict, recall or withdraw an EHR system
whose manufacturer does not cooperate or gives incomplete information (Art
43(5)), and a missing technical documentation, declaration of conformity, CE
marking or registration is a finding of non-compliance in its own right (Art
45(1)). The technical documentation and the declaration are being built under
[#525](https://github.com/FerroHEALTH/FerroFED/issues/525).
