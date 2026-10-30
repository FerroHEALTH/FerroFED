---
name: ferroui-lookup
description: >
  Finds and reads the authoritative FerroUI guidance for a topic (a
  component, a script module, the document and the shell, forms and post
  routes, the session watch, the search box, tokens, icons, words,
  accessibility) in the FerroUI repository at the tag of the console's
  ferrohealth-ui pin. Use before implementing or reviewing any operator
  console (app/ferrofed-console) behaviour the rule file does not fully
  settle, or when a "how does FerroUI do X" question comes up.
allowed-tools: Read, Grep, Glob, Bash
argument-hint: "<component / module / shell / form / session / search / token topic>"
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# /ferroui-lookup

Answer FerroUI questions from the library's own text at the pinned tag,
never from memory: the library moves between releases, and its rules on
`main` describe parts the pinned version may not have. The distilled rules
particular to this console live in `.claude/rules/console-ui.md`; this skill
goes back to the source when a case is not covered or needs full context.

## Procedure

1. **Read the library at the pinned tag** (a shared cache that survives
   sessions):

   ```bash
   FUI=~/.cache/ferrofed/ferroui
   [ -d "$FUI/.git" ] || git clone https://github.com/FerroHEALTH/FerroUI "$FUI"
   git -C "$FUI" fetch --tags -q
   PIN=$(grep -E '^ferrohealth-ui = ' Cargo.toml | grep -oE '[0-9]+\.[0-9]+\.[0-9]+')
   git -C "$FUI" show "v$PIN:docs/consoles.md"
   git -C "$FUI" show "v$PIN:.claude/rules/console-ui.md"
   git -C "$FUI" show "v$PIN:crates/ferrohealth-ui/CLAUDE.md"
   ```

2. **Route to the owning text:**
   - how a console builds, the document, the assets, the policy, an action
     that answers on its page, the search box, the session's end:
     `docs/consoles.md`
   - a 0.5 part and what replaced it: `docs/migration-0.6.md`
   - a component's markup, roles and keys: `crates/ferrohealth-ui/src/view/<part>.rs`
     and its template under `crates/ferrohealth-ui/templates/`
   - a module's behaviour: `crates/ferrohealth-ui/scripts/<job>.js`
   - the words a component draws: `crates/ferrohealth-ui/locales/en/`
   - the design of a page and a component's ID: `design/consoles/ferrofed/`
     and `design/COMPONENTS.md`
   - a worked example: `app/ferrohealth-ui-gallery/src/` and its journeys
     under `app/ferrohealth-ui-gallery/tests/it/`

3. **Grep** the exact name (`danger_form_dialog`, `Countdown`, `RepeatRow`,
   `script::MARK`) across the tree with `git -C "$FUI" grep -n <name> "v$PIN"`
   when the routing is not obvious.

4. **Answer with citations** (the file at the tag, plus the heading or the
   item). When FerroUI lacks the part or its rule is wrong, the answer is an
   issue to file in FerroHEALTH/FerroUI with its `scripts/gh/fields.sh new`,
   never a local component, stylesheet override or script. If the answer
   belongs in the standing rules, propose the addition explicitly: to
   FerroUI as an issue when it holds for every console, to
   `.claude/rules/console-ui.md` when it is particular to this one.
