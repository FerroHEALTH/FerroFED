<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Post-market procedures

The working checklist the manufacturer of FerroFED, Cadasto B.V., follows
for a complaint, a non-conforming version and a serious incident, under
Regulation (EU) 2025/327 Art 30(1)(i) to (o), Art 30(5), Art 43(5), Art 44
and Art 45(1). The public account, with the quotations, is the book's
[Complaints and incidents](https://ferrofed.eu/docs/evaluate/post-market.html)
page; this page is what a maintainer does, step by step. The Regulation fixes
the duties and the three-day limit; the steps that carry them out are our own
design.

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
   and its timetable into `action`. Withdrawing a release is scripted under
   [#706](https://github.com/FerroHEALTH/FerroFED/issues/706).
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
   date and hour of awareness. Ask the deployment for the version it runs,
   how it is deployed and what happened; the report bundle that answers this
   in one step is planned under
   [#705](https://github.com/FerroHEALTH/FerroFED/issues/705).
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

## A request from an authority

A request for information or documentation is answered in an official
language of the Member State concerned (Art 30(1)(l)), on paper or
electronically, in a language the authority understands (Art 30(5)). The
manufacturer keeps the request and the answer. The technical documentation
and the declaration of conformity the answer draws on are built under
[#525](https://github.com/FerroHEALTH/FerroFED/issues/525). Art 43(5) lets an
authority restrict, recall or withdraw FerroFED when the manufacturer does
not cooperate or answers incompletely.
