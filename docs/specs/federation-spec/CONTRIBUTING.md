# Contributing

Thanks for your interest! This specification is a **work in progress**, and input at
every level is genuinely welcome: typo fixes, wording clarifications, technical
objections, alternative bindings, implementation experience - all of it. Don't hesitate
to open an issue even for a half-formed thought; discussion is the point of publishing
this as a draft.

## Where discussion happens

- **GitHub issues** - questions, comments, proposals, disagreements. No template
  required; just say what you mean.
- **Pull requests** - for concrete text changes, small or large. A PR that sparks a
  discussion and gets reworked is a success, not a failure.

## How to reference the spec

To keep comments unambiguous, please cite:

- **Sections** by their § number, e.g. *§7.1 (the rewrite rule)* or *§11.2 (HTTP status
  mapping)*.
- **Normative requirements** by their N-number, e.g. **N7** (they live in §6, one anchor
  per requirement, so `…/requirements.html#n7` deep-links).
- **Conformance points** by their CP-number, e.g. **CP-14** (§17, anchors `#cp-14` etc.).

## How to cite the spec *inside* the spec

Two rules, both learned the hard way — v0.3 shipped with four references that
pointed at sections which do not exist, and nothing caught them:

- **Cite a subsection as `xref:page.adoc#anchor[§n.n]`**, never as bare prose
  `§n.n`, wherever the target has an anchor. Add one if it does not.
- **Never put a § number in link text that the `xref:` does not resolve to.**
  `xref:annex-b-dutch-gf.adoc[Annex B §B.7]` resolves the *page*, so Antora
  passes it even when §B.7 does not exist. The subsection number in the link
  text buys none of the xref's safety.

**Run `tools/check-refs.sh` before opening a PR.** It resolves every prose
`§n.n` against the headings that actually exist and is the half of the
reference gate Antora does not cover. `tools/traceability.sh` reports the
requirement → conformance-point → track closure; a requirement scored by
neither must be listed in `tools/traceability-exceptions.txt` with a reason.
Both also run in CI on every push and pull request
(`.github/workflows/validate.yml`).

## The published JSON Schemas

Two wire structures have machine-readable contracts in
`modules/ROOT/attachments/`, published alongside the site and validated in CI by
`tools/check-schemas.sh` (run it before opening a PR):

| Schema | Governs | Upstream |
| --- | --- | --- |
| `federated-result-set.schema.json` | The AQL result envelope (§9) | **openEHR ITS-REST Release-1.1.0.** The envelope *is* a `RESULT_SET`; the schema inlines a constrained subset of the ITS-REST definitions rather than `$ref`-ing them by URL, so CI does not depend on a third-party site being reachable. |
| `options-root.schema.json` | The `OPTIONS {base}/` body (§7a.2) | **None.** Entirely federation-defined, which is why it needs a schema more than the envelope does. |

**Changing the envelope or the `OPTIONS` body means changing the schema in the same
PR.** A schema that lags the prose is worse than no schema: it makes a stale contract
look enforced. Three rules follow from that:

- **Add the schema change to the same commit as the prose change.** `check-schemas.sh`
  validates the spec's own `[source,json]` examples, so a prose change that updates an
  example will fail CI until the schema agrees — which is the gate working, not a
  nuisance to route around.
- **Do not tighten a schema beyond what the prose says.** If the schema needs a
  constraint the prose does not state, the prose is what is incomplete. Fix it there
  first; writing these schemas is how §9.5's unconditional `latency_ms` MUST and two
  missing `OPTIONS` keys were found.
- **Never add federation constraints inside `$defs/itsRest`.** That subtree is a
  restatement of someone else's standard, and its value is that a reader can tell at a
  glance which rules this specification owns. Constrain at the federation level instead.

Re-binding to a later ITS-REST release is a deliberate act, not a refresh: re-fetch
`computable/OAS/query-validation.openapi.yaml` for the new release, diff `ResultSet`,
`ResultSetMetadata`, `ResultSetColumn` and `ResultSetRow` against the inlined subset, and
update §9.1, N17 and the schema's `$comment` together.

## Bumping the version

Most version literals come from attributes in `antora.yml`
(`spec-version`, `spec-status`, `spec-date`), which `index.adoc` interpolates.
Four sites cannot, and must be bumped by hand together:

| Site | Why it cannot be an attribute |
| --- | --- |
| `antora.yml`'s own `version:` | The Antora component version. It carries the **minor** version only (`0.9`) — a patch release replaces its predecessor in place, so there is no version selector and no stale URLs. Bump it only for a minor release. |
| `package.json`'s `version` | Outside Antora's attribute scope. |
| `README.md` | GitHub Markdown; no attribute expansion. |
| `rest-facade.adoc`'s `"spec_version": "0.9"` | Inside a `[source,json]` block. Substituting there needs `subs="attributes+"`, which makes the example non-copy-pasteable. It is also **deliberately `major.minor`**: the field reports the wire contract a gateway implements, and patch releases do not change it — see §7a.2. Leave it alone for a patch release; bump it for a minor one. `options-root.schema.json` enforces the `major.minor` shape with a `pattern`, so a three-component value fails CI. |

A fifth site lives in another repository: the **reference implementation**
(https://github.com/syntaric/openehr-federation-ref) tracks the spec version in its
`pom.xml`, `README.md`, `CHANGELOG.md`, `docs/conformance.md`, and the `spec_version`
literal its `ConformanceController` emits (asserted by `ConformanceOptionsIT`). A minor
release that moves `spec_version` breaks that test until both repositories are bumped
together.

## What to edit

The **AsciiDoc pages under `modules/ROOT/pages/` are the source of truth.** Edit those.
Please don't edit any exported artifact (HTML, DOCX, PDF renderings that may circulate) —
they are generated from, or superseded by, these pages.

A note on requirement language: the key words MUST / SHOULD / MAY follow RFC 2119, and
where a requirement names an IHE profile, the profile is a **proposed binding** - the
obligation is normative, the named profile is open to review (see *Reader's guide and
terminology*). Challenging a proposed binding is an entirely legitimate contribution.

## Diagrams

Diagrams are authored in **PlantUML**. To change one:

1. Edit the source in `diagrams/*.puml`.
2. Re-render the PNG with PlantUML (not shipped in this repo; download
   `plantuml.jar` from https://plantuml.com/download):

   ```sh
   java -jar plantuml.jar -tpng -o ../modules/ROOT/images diagrams/*.puml
   ```

3. Commit both the `.puml` source and the re-rendered PNG in
   `modules/ROOT/images/` together.

## Licensing of contributions

This specification is dedicated to the public domain under
[CC0 1.0 Universal](LICENSE). By contributing you agree that your contribution is
published under the same terms.
