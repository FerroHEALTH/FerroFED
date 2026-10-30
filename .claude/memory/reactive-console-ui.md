---
name: reactive-console-ui
description: The consoles are server-rendered on FerroUI 0.6 (owner, 2026-10-29, FerroUI #510); an in-page change keeps its state through a module, and a navigation loads one document. Supersedes the 2026-10-25 ruling for client navigation
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The owner ruled on 2026-10-29 (FerroUI #510) that every FerroHEALTH console
moves to server-rendered HTML with Askama and a little plain JavaScript, with
no WebAssembly and no Leptos hydration. A link loads one complete document
the server drew with the state it read; an in-page change (a tab, a tree
level, a menu, a dialog) is a FerroUI module toggling what the server drew,
so the expansion, the scroll and the selection stay where they were. This
supersedes the ruling of 2026-10-25, which made WebAssembly the normal path
and client navigation the target.

**Why:** the hydrated mode of FerroUI 0.5 flickered on every button, lost
what was selected and redrew whole pages for a tree node (FerroUI #510,
measured on FerroEHR's viewer). FerroFED's console moved with FerroUI 0.6.0
(#1457).

**How to apply:** judge a UI change by the interaction it gives: in-page
state is kept by a module and never lost to a document load; a page the
server could draw is drawn by the server; every page reads and every action
posts with the modules refused.
