---
name: perl-edit-pitfalls
description: "Editing with perl from Bash: heredoc-quoted literals, never q{} or s{}{} around text with braces; read the current text right before matching, since rustfmt reflows it; a stale rmeta after fast edits needs cargo clean -p; carried from FerroTERM (2026-09-03)"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Three failures repeated on FerroTERM on 2026-09-03: a `q{...}` or
`s{...}{...}` perl string containing Rust braces breaks the script; a
replacement written from memory fails once `cargo fmt` has reflowed the
target; and an edit in the same second as the previous build leaves a stale
`.rmeta`, so dependents report "unresolved import" for an item that exists.

**How to apply:** put every literal in a perl `<<'END'` heredoc and match with
`\Q...\E`, dying on a miss so nothing half-applies; prefer the Edit tool for
exact multi-line replacements; after a perl edit that changes a public API,
run `cargo clean -p <crate>` before checking dependents when the error names
an item that is plainly there.
