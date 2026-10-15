<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Post-market procedures

The working checklist the manufacturer of FerroFED, Cadasto B.V., follows
for a complaint, a non-conforming version and a serious incident, under
Regulation (EU) 2025/327 Art 30(1)(i) to (o), Art 30(5), Art 43(5), Art 44
and Art 45(1), and for an actively exploited vulnerability or a severe
incident under Regulation (EU) 2024/2847 (the CRA) Art 14, and the register
of economic operators of EHDS Art 35 and CRA Art 23. The public
account, with the quotations, is the book's
[Complaints and incidents](https://ferrofed.eu/docs/evaluate/post-market.html)
page; this page is what a maintainer does, step by step. The Regulations fix
the duties and the time limits; the steps that carry them out are our own
design.

## Who does what

- **The owner** acts for Cadasto B.V. as FerroFED's manufacturer. The owner
  decides whether an event is reportable, submits every notification to the
  authorities, approves every advisory and user notice, and is the contact
  the CSIRT and the market surveillance authorities deal with.
- **The maintainers** triage reports, draft the notifications, the
  advisories and the fix, and tell the owner at once of anything that may
  be an actively exploited vulnerability, a severe incident or a
  serious incident.

The single point of contact is `info@cadasto.com` (Art 30(1)(g)). It is
named in the running system from `app/ferrofed-registry/src/manufacturer.rs`,
and a change of address or contact changes that file, `LICENSE` and the image
labels in one pull request; the tests fail on any that disagree.

## The channels

| What | Channel | Who reads it |
|---|---|---|
| A vulnerability, or a patient identifier reaching a node | GitHub private vulnerability reporting | the maintainers |
| A possible serious incident | `info@cadasto.com`, subject "FerroFED incident" | Cadasto B.V., forwarded to the maintainers the same day |
| Any other complaint | `info@cadasto.com`, subject "FerroFED complaint", or a GitHub issue | Cadasto B.V. and the maintainers |

Every complaint, whatever the channel, gets a row in
[`docs/registers/complaints.tsv`](registers/complaints.tsv) within one
working day of receipt, with no personal data and no patient data in it. A
vulnerability gets its row when its advisory is published.

## A complaint

1. Enter the row: `received`, `channel`, `versions`, a one-sentence
   `summary`, and `record`, the tracker issue or advisory that carries the
   work.
2. Classify it. `complaint` when FerroFED behaves as documented and the
   report asks for something else; `non-conformity` when a released version
   misses an Annex II item or a Chapter III obligation, which also opens a row
   in the register of non-conforming versions; `serious-incident` when it
   meets Art 2(2)(r), which starts the incident clock below at once.
3. Answer the person through the channel they used.
4. Close the row with its `outcome` and `closed` date when the record closes.
5. Tell each distributor, importer and authorised representative of every
   new row (Art 30(1)(o)).

## A non-conforming version

Art 30(1)(i) asks for corrective action "without undue delay" once the
manufacturer considers, or has reason to believe, that a version does not
conform.

1. Enter the row in
   [`docs/registers/non-conforming-versions.tsv`](registers/non-conforming-versions.tsv):
   `found`, `versions`, `requirement`, `summary`, and whether it is a serious
   incident.
2. File the fix on the tracker as a `Bug` at priority `Urgent`, in the
   current milestone, and ship it as a patch release. A published release is
   immutable, so the fix ships forward ([`release.md`](release.md)).
3. Decide between correction, recall and withdrawal, and write the decision
   and its timetable into `action`. A withdrawal runs
   `scripts/release/withdraw.sh` once the correcting release is out
   ([`release.md`](release.md#withdrawing-a-release)). It refuses a version
   with no row here, moves the `<major>.<minor>` and `latest` image tags to
   the correcting release, publishes the advisory, writes the upgrade note,
   lists the version as withdrawn in `SECURITY.md`, fills `users_told` and
   adds the withdrawal to `action`.
4. Tell the national authorities of each Member State where the version was
   made available or put into service: the non-conformity, the corrective
   action with its timetable, and the date of conformity, recall or
   withdrawal. Record the date and the Member States in `authorities_told`.
5. Tell distributors, the authorised representative, importers and users
   (Art 30(1)(j)): a security advisory where the finding is a vulnerability,
   the release notes of the correcting version, and a direct message where
   the user is known. Record the date and the route in `users_told`.
6. Close the row with `corrected_in` and `closed`.

## A serious incident

The clock starts when the manufacturer becomes aware, and the report is due
"immediately after the manufacturer has established a causal link ... or the
reasonable likelihood of such a link and, in any event, not later than three
days after" that (Art 44(7)).

1. **Day 0, on awareness.** Enter the complaint row as `serious-incident`
   and the non-conforming row with `serious_incident` set. Write down the
   date and hour of awareness. Ask the deployment for the archive
   `ferrofed report` writes on the host the gateway runs on (see below), and
   for how it is deployed and what happened. The archive names the version,
   the commit, the target, the bindings, the pins and the release
   attestations to verify, so it carries "the data necessary for the
   identification of the EHR system concerned" (Art 44(5)). Attach it to the
   report.
2. **By day 3 at the latest.** Report to the market surveillance authority of
   each Member State where the incident occurred and where FerroFED is placed
   on the market or put into service. The report says what happened, which
   versions are involved, the causal link as far as it is established, and
   the corrective action taken or envisaged. A report sent before the cause
   is known says so, and is completed when it is.
3. **NIS 2, separately.** The report is "without prejudice to" Directive
   (EU) 2022/2555. Whether a notification under it is due, from the
   manufacturer or from the deployment, is checked the same day and never
   assumed to be covered by this report.
4. **Harm.** Where an authority finds harm to health or safety, give the
   affected person or user the information and documentation at once (Art
   44(3)), within data protection rules.
5. Carry on as for a non-conforming version from step 2, across every copy
   placed on the market in the Union (Art 44(4)).
6. **CRA, separately.** Check the same day whether the event is also an
   actively exploited vulnerability or a severe incident under the CRA (the
   next section). Both sets of reports are made, each on its own clock.

## An actively exploited vulnerability or a severe incident (CRA Art 14)

CRA Art 14 applies from 11 September 2026 (Art 71(2)) to every release,
those placed on the market before 11 December 2027 included (Art 69(3)).
The clock starts when the manufacturer becomes aware. Every notification
goes through the single reporting platform (Art 16(1)), at the end-point of
the CSIRT the Netherlands designated as coordinator, the Member State of
Cadasto B.V.'s main establishment, and is simultaneously accessible to
ENISA (Art 14(1), (3), (7)).

1. **Hour 0, on awareness.** Write down the date and hour. Decide whether
   it is an actively exploited vulnerability, with "reliable evidence that a
   malicious actor has exploited it in a system without permission of the
   system owner" (Art 3(42)), or a severe incident having an impact on the
   security of FerroFED (Art 14(5)). The owner decides. Open a draft GitHub
   security advisory, which holds the record from here on.
2. **Within 24 hours: the early warning** (Art 14(2)(a), (4)(a)). Name the
   Member States where Cadasto B.V. knows FerroFED has been made available,
   from its contracts, as the register of economic operators below records
   them. For an incident, say whether it is suspected of being
   caused by unlawful or malicious acts.
3. **Within 72 hours: the notification** (Art 14(2)(b), (4)(b)). The
   releases concerned, the nature of the exploit and the vulnerability, or
   an initial assessment of the incident; the corrective or mitigating
   measures taken and those users can take; and how sensitive the
   information is. Say when a security update or user guidance is
   expected: within 72 hours, that is the ground for a delay that
   Delegated Regulation (EU) 2026/881 ties to the manufacturer (its Art
   3(a)). Ask for the dissemination to be delayed only on the grounds Art
   16(2) and that Regulation (its Art 3 to 5) allow; the CSIRT decides, and
   the Regulation puts no duty on the manufacturer.
4. **Tell the users** (Art 14(8)). Publish the advisory once users can act
   on it: the releases affected, the mitigation a deployment can apply now,
   and the fixed release once there is one. For an actively exploited
   vulnerability the advisory does not wait for the fix when a mitigation
   exists. Write directly to every user known from a contract. Record the
   date and the route.
5. **Fix forward.** File the fix as a `Bug` at priority `Urgent` in the
   current milestone and ship it as a patch release, as for a non-conforming
   version.
6. **Intermediate reports.** Answer any request of the CSIRT for a status
   update (Art 14(6)).
7. **The final report** (Art 14(2)(c), (4)(c)). For a vulnerability, no
   later than 14 days after the corrective or mitigating measure is
   available: the description, severity and impact, the malicious actor
   where known, and the update. For an incident, within one month after
   the notification of step 3: the description, severity and impact, the
   type of threat or root cause, and the mitigation applied and ongoing.
8. **EHDS and NIS 2.** Check the same day whether the event is also a
   serious incident (the section above) and whether a NIS 2 notification is
   due. Neither is covered by the CRA notifications.

## The register of economic operators

EHDS Art 35 has economic operators "on request, identify ... to the market
surveillance authorities, for 10 years from the date when the last EHR
system covered by the EU declaration of conformity has been placed on the
market: (a) any economic operator that has supplied them with an EHR system;
and (b) any economic operator to which they have supplied an EHR system".
CRA Art 23(1) asks for "the name and address" of each, the second "where
available", and Art 23(2) has the information presented "for 10 years after
they have been supplied with the product with digital elements and for 10
years after they have supplied" it. Decision A73 (#654) meets both with a
register of every economic operator Cadasto B.V. supplies under a contract.
The steps below are our own design; no article prescribes them.

### What it holds

One row per supply, in the columns of
[`docs/registers/economic-operators.tsv`](registers/economic-operators.tsv):
the direction (`supplied-to` for Art 35(b) and Art 23(1)(b), `supplied-by`
for Art 35(a) and Art 23(1)(a)), the date, the operator's name and address,
its role under Regulation (EU) 2019/1020 Art 3(13), the versions and the
artefacts supplied, the Member States where the contract has FerroFED made
available or put into service, the contract's reference, and `keep_until`.
The Member States column is also where step 2 of the CRA section above
reads "the Member States on the territory of which the manufacturer is
aware that their product with digital elements has been made available"
(CRA Art 14(2)(a)).

### Where it is kept

The rows name customers and their addresses, so the register is kept
outside the public repository, as a private copy of that file under git,
read by the owner and by whoever the owner names. The public copy holds the
column row and no entry, and `scripts/checks/economic-operators.sh` fails CI
if a row is ever added to it.

### Who and when

The owner keeps the register and enters every row. A row is entered within
one working day of a supply: when a release is delivered under a contract,
or when a contract gives an operator a release to make available or put
into service. A release delivered later under the same contract is a new
row, because rows are appended and never rewritten.

EHDS Art 35 applies from 26 March 2027, the general date of Art 105, on the
reading the book's regulatory status page asks counsel to confirm. CRA Art
23 applies from 11 December 2027 (Art 71(2)), and Art 69(2) holds a release
placed on the market before that date to it only once the release is
"subject to a substantial modification". The register is kept from the
first supply all the same, so the 10 years can be shown whichever date
applies.

### For how long

A row stays at least 10 years after its supply (CRA Art 23(2)) and at least
10 years after the last EHR system covered by the declaration of conformity
of its versions has been placed on the market (EHDS Art 35). The second date
is not known when the row is entered, so `keep_until` stays empty and is
filled only once both are known; a row with an empty `keep_until` is never
removed. A published release cannot be changed or deleted
([`release.md`](release.md)), so when the last placing on the market of a
version has happened is a decision for the owner (below).

### The check

Before each commit to the private register, and before answering a request,
the owner runs `scripts/checks/economic-operators.sh --register <path>`. It
refuses a malformed row, a `keep_until` less than 10 years after the supply,
and, across the file's git history, a row removed before its `keep_until`
has passed or changed in any column but an empty `keep_until`.

### A public download

Each tagged release is published on GitHub and as container images for
anyone to download, and the download is not gated (A73). Cadasto B.V. does
not know who downloads a release, so a download is no row; the release is
stated to be supplied for non-production use and for non-commercial
production use under the licence. EHDS Art 35 has no "where available"
qualifier, unlike CRA Art 23(1)(b). If counsel holds that an anonymous
production user is an economic operator Cadasto B.V. supplied, for example
one that puts FerroFED into service under EHDS Art 26(2), the A73 fallback
applies: downloads move behind a registration, and each registration is a
row. Until that answer, whether a download is a supply to an identified
operator is undecided, and the register does not decide it.

### A request from a market surveillance authority

The owner answers it as the next section says. Run the check, then select
the rows of the versions and the period the request names, and give for
each the operator's name and address, its role, the versions and the date
of supply. Say plainly that the versions were also published for download,
and that their downloaders are not known. Keep the request, the answer and
the rows given.

### Open for the owner

These facts and decisions are the manufacturer's, and none is recorded in
the repository:

1. Where the private register is kept, and who other than the owner reads
   it.
2. The rows: every supply under a contract so far, with the operator's name,
   address and role, the versions, the artefacts, the Member States and the
   contract reference.
3. Whether any economic operator has supplied Cadasto B.V. with an EHR
   system or a product with digital elements that is part of what it
   supplies (the `supplied-by` rows).
4. Counsel's answer on whether an anonymous production user of a public
   download is an economic operator Cadasto B.V. supplied, which decides
   whether the A73 registration fallback applies.
5. When the last placing on the market of a version under its declaration
   of conformity has happened, which fixes each row's `keep_until` under
   EHDS Art 35.

## A request from an authority

A request for information or documentation is answered in an official
language of the Member State concerned (Art 30(1)(l)), on paper or
electronically, in a language the authority understands (Art 30(5)). The
manufacturer keeps the request and the answer. The technical documentation
and the declaration of conformity the answer draws on are built under
[#525](https://github.com/FerroHEALTH/FerroFED/issues/525). Art 43(5) lets an
authority restrict, recall or withdraw FerroFED when the manufacturer does
not cooperate or answers incompletely.

## The report archive

`ferrofed report` writes one uncompressed tar archive, by default
`ferrofed-report-<UTC time>.tar`, that a deployment attaches to a complaint
or an incident report. It reads the configuration the way `serve` does and
asks the gateway on the same host for its live state. A part it cannot read
is named in the manifest with the reason, never left empty. The book's
[Complaints and incidents](https://ferrofed.eu/docs/evaluate/post-market.html#the-report-archive)
page lists the files. Before forwarding an archive, check its `missing` list:
a stopped gateway has no live parts, and the incidents need an operator
token (`--operator-token-file`).

The archive holds no patient identifier, `ehr_id`, clinical payload,
credential, personal name, header value or URL userinfo. If one is ever found in an archive, that is a
vulnerability: report it as one, and delete the archive.
