<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Getting help

Three destinations, and they are not interchangeable. Picking the right one is
the difference between an answer and a thread nobody is paged for.

## I have a question

Start with the documentation at <https://ferrofed.eu/docs/>: installing and
running the gateway, its configuration, the client contract, and what it
claims. The design of record is
[`docs/architecture.md`](docs/architecture.md), every decision with its
citation, and the
[releases page](https://github.com/FerroHEALTH/FerroFED/releases) carries each
release with its notes. The governing text is the
[Federation Tier with AQL specification](https://syntaric.github.io/openehr-federation-spec/).

If those do not answer it, **open a GitHub issue** through the
[issue chooser](https://github.com/FerroHEALTH/FerroFED/issues/new/choose):
whether an approach fits, what a requirement means in this implementation, or
why a design is the way it is.

There is no commercial support offering, no service-level agreement, and no
paid tier. Answers come when the maintainer is at a keyboard
([MAINTAINERS.md](MAINTAINERS.md) is honest about how many keyboards that is).

## I found a defect

**[Open an issue](https://github.com/FerroHEALTH/FerroFED/issues/new/choose)**
when something is wrong, missing, or contradicts one of the specifications
this project answers to: the Federation Tier with AQL specification, openEHR
AQL and ITS-REST, or a bound IHE profile.

The reports that get fixed fastest carry:

- the version (the release tag or commit), and how it is deployed, including
  which CDRs it federates and which identity service it resolves against;
- the query and the request as sent, and the answer as returned, verbatim;
- what the specification says should have happened, with the section, the
  `N#` requirement or the `CP-#` conformance point if you have it. A citation
  turns a disagreement into a defect.

**Never include patient data.** Redact it, or synthesize an equivalent case.
An identifier that would let anyone find a real person does not belong in an
issue, even pseudonymised.

**A specification-conformance report is the most valuable kind here.** The
implementation is never presumed correct because it was written to the
specification; the specification text is the authority. The specification's
reference implementation is prior art, so "the reference implementation does
it differently" is not by itself a defect; a citation is.

## I found a vulnerability

**Do not open a public issue.** Follow [SECURITY.md](SECURITY.md): report
privately through
[GitHub private vulnerability reporting](https://github.com/FerroHEALTH/FerroFED/security/advisories/new).
A gateway that leaks a patient identifier to a node, or that answers a query
it should have refused, is a vulnerability.

**A vulnerability in a dependency, or in a CDR or identity service you
deployed alongside FerroFED, goes to that project**, not here, unless it has a
FerroFED-specific impact.

## I want to change something

[CONTRIBUTING.md](CONTRIBUTING.md) is the practical guide: what helps most,
the gates every pull request must pass, and the hard rules.
[GOVERNANCE.md](GOVERNANCE.md) is how the decision gets made and how someone
becomes a maintainer.

## What you are entitled to

Nothing, and that is worth saying plainly. FerroFED is provided as-is under the
Business Source License 1.1, with no warranty. Read the [LICENSE](LICENSE),
which says exactly that in the language that binds. Everything above describes
what the project *intends* to do, and the intent is sincere; none of it is a
contractual commitment, and only the security-report windows in SECURITY.md are
stated as promises at all.

If your deployment needs a stronger guarantee than a one-person project can
give, the honest options are to fork and maintain, or to fund the capacity that
would change the answer.
