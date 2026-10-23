<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Changelog fragments

A pull request with a user-visible effect adds one file here instead of
editing `CHANGELOG.md`, so two pull requests never touch the same lines.

- **Name:** `<issue>-<kebab-slug>.<section>.md`, for example
  `598-changelog-fragments.changed.md`. `<section>` is `upgrade`, or a Keep a
  Changelog 1.1.0 section in lower case: `added`, `changed`, `deprecated`,
  `removed`, `fixed` or `security`. The one other name,
  `<issue>-<kebab-slug>.security-reason.md`, is the reason paragraph of
  § Security entries.
- **Upgrade notes:** an `upgrade` fragment says what an operator must change
  to upgrade: a key removed, renamed or made mandatory, a value now refused,
  a file to move. The release lists it under "Upgrade notes", before every
  other section. A change that makes the previous release's example
  configuration fail `config check` needs one, and
  `scripts/checks/upgrade-notes.sh` holds CI to it.
- **Content:** the entry exactly as it reads in `CHANGELOG.md`, one or more
  list items that open with `- ` and continue on lines indented by two spaces,
  with no heading and no blank line inside. A fragment carries no SPDX header:
  its text moves into `CHANGELOG.md`, which carries the header.

`scripts/release/changelog.sh --check` validates every fragment, and CI runs it.
The release cut runs `scripts/release/changelog.sh --assemble <version> <date>`,
which moves the fragments into `CHANGELOG.md` under the new version and
deletes them. `scripts/release/changelog.sh --preview` prints `CHANGELOG.md`
as the cut would write it, with the fragments under `[Unreleased]`. This
README is not a fragment.

## Security entries

Two rules of Regulation (EU) 2024/2847 hold every Security section, and
`scripts/checks/changelog-security.sh` checks both: CI runs it on every pull
request over the preview of `[Unreleased]`
(`scripts/checks/changelog-unreleased.sh`), and the release `plan` job over
the section it cuts (`scripts/release/plan.sh`). A section released before
the rules is never checked again.

- **Every entry names its advisory** (Annex I Part II(4), (8)). A
  `security` fragment carries the `GHSA-` id of the FerroFED security
  advisory drafted for the fix and published with the release
  (`docs/post-market.md`, A fixed vulnerability's advisory), or, for a
  dependency finding an OpenVEX statement shows does not affect FerroFED,
  the path of that statement under `security/vex/`, which must exist. A
  RUSTSEC or DSA id alone is not enough: name it beside the advisory.
- **A security fix ships alone, or says why not** (Annex I Part II(2)). A
  release whose Security section sits beside any other section opens that
  section with one paragraph, `Shipped with functional changes because:`
  and the reason on the same line, naming why a security-only patch of the
  newest release was not the way to ship it. The paragraph is a fragment of
  its own, `changelog.d/<issue>-<kebab-slug>.security-reason.md`: plain text
  that opens with that prefix, with no list item and no blank line inside.
  At most one exists at a time; the cut places it first under
  `### Security` and refuses it when no Security entry is pending. Whoever
  adds the first Security fragment beside other pending changes adds it,
  and whoever cuts the release rereads it.
