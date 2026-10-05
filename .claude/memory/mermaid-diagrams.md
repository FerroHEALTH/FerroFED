---
name: mermaid-diagrams
description: "The design of record carries mermaid diagrams; render every fence with mermaid-cli against the installed Chrome before a pull request, and keep semicolons out of sequence messages; carried from FerroBRIDGE (2026-09-12)"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Owner request on FerroBRIDGE, 2026-09-12: the architecture explains the
design with mermaid diagrams as well as text, because a picture is faster to
check than prose. GitHub and an mdBook site with `mdbook-mermaid` both render
them. FerroFED's `docs/architecture.md` follows the same rule; the request
flow (resolve, fan out, merge, route) is the obvious first diagram.

**Why:** FerroBRIDGE's first pass shipped a diagram that did not parse (a
semicolon in a sequence message ends the statement), and GitHub shows a parse
error where the picture should be.

**How to apply:** extract every ```` ```mermaid ```` fence and render it with
`npx -y -p @mermaid-js/mermaid-cli mmdc -p <puppeteer.json> -i x.mmd -o x.svg`,
with the puppeteer config's `executablePath` pointing at the installed Google
Chrome. Use `<br/>` for line breaks in labels, never `\n`; keep `;` out of
sequence messages; quote any label carrying parentheses, colons or braces.
The specification's own diagrams are PlantUML sources under the vendored
`diagrams/`; cite them, never redraw them as if they were ours.
