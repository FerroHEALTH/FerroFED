---
paths: ["app/ferrofed-console/**"]
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Operator console rules (`app/ferrofed-console`)

**FerroUI leads the UI.** The console draws with `ferrohealth-ui`, pinned
exactly in the root `[workspace.dependencies]` (today `=0.6.2`), and the
FerroUI repository (FerroHEALTH/FerroUI) is the source for its components,
design, tokens, icons, script modules, accessibility bar, interaction
contract, words and quality control. Read these first, at the git tag that
matches the pin (today `v0.6.2`):

- [`docs/consoles.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.6.2/docs/consoles.md):
  the consumer guide, how a console uses the library.
- [`docs/migration-0.6.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.6.2/docs/migration-0.6.md):
  what replaced each 0.5 part, and why (FerroUI#510).
- [`.claude/rules/console-ui.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.6.2/.claude/rules/console-ui.md):
  the component rules of every FerroHEALTH console. The section numbers this
  file cites (FerroUI §N) are those of the `v0.6.2` copy.
- [`crates/ferrohealth-ui/CLAUDE.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.6.2/crates/ferrohealth-ui/CLAUDE.md):
  the crate's own discipline.
- [`app/ferrohealth-ui-gallery`](https://github.com/FerroHEALTH/FerroUI/tree/v0.6.2/app/ferrohealth-ui-gallery):
  the worked example of a console on 0.6, its no-script and navigation
  journeys among them.
- [`design/consoles/ferrofed/`](https://github.com/FerroHEALTH/FerroUI/tree/v0.6.2/design/consoles/ferrofed):
  the reviewed design of this console. Every page matches its mockup at
  1440 and 390 px, light and dark. FerroUI's `design/` is the only design
  source; a design folder anywhere else is stale.

A component this console needs and the library lacks, or one that behaves
wrongly, is an issue filed in FerroHEALTH/FerroUI with its
`scripts/gh/fields.sh new`, naming the existing parts it considered, linked
from the work here: never a local copy, a shadow component, a stylesheet
override or a script in this repository. Until FerroUI ships it, the console
uses what the library has.

This file keeps only what is particular to the operator console: its
routes and its forms, the BFF and its session, the gateway client and the
admin actions, the security headers, the journeys and screenshots, and the
console's own words.

## 0. The mandates (absolute)

- **Every page is one document the server draws whole.** A handler reads
  what the page draws of the gateway, then draws it through
  `ferrohealth_ui::view::shell::document` (`crate::app::answer`), so the
  page arrives complete and a link loads the next page as a document
  (FerroUI `docs/consoles.md`, How a console builds). FerroUI's script
  modules add behaviour on top of what the server drew: every page reads
  and every action posts with them refused. The console writes no script,
  no inline `style` attribute and no `<style>` element of its own (FerroUI
  §0), and links no Leptos crate and no WebAssembly glue
  (`app/ferrofed-engine/tests/it/architecture.rs`,
  `the_operator_console_draws_every_page_on_its_server`).
- **The console is a client of the gateway and nothing else.** It reaches
  the gateway over HTTP, on the public surface any client uses, through
  `crate::gateway` and the `openehr-its` client, and links no part of the
  gateway (`the_operator_console_reaches_the_gateway_over_http_alone`). A
  screen that needs what the gateway does not expose gets a read-only
  operator API on the gateway first, as its own issue.
- **The console stores no clinical data and holds no token in the
  browser.** Every page is `Cache-Control: no-store`; nothing is kept in
  browser storage, a service worker or a URL. The session is server-side
  (`crate::session`): the browser carries one opaque `HttpOnly` session
  cookie and one `HttpOnly` cookie per preference (`crate::prefs`), each
  holding a word and nothing else. A secret the operator types lives only
  in a password field that is never filled, a type that zeroes on drop, and
  the request that hands it to the replica.
- **No patient identifier in a URL, the browser history, or a log**
  (`.claude/rules/identifier-hygiene.md`). A query and its parameters are a
  `POST` body; a `GET` of the query page never runs one.
- **Every state-changing request is a same-origin `POST`.** The server's
  `same_origin_only` layer refuses any other method from another origin
  (`Sec-Fetch-Site`, else `Origin`/`Referer`) before anything reads the
  body, and every action checks the session itself before it asks the
  gateway or a replica.

## 1. Pages and forms

- **A page is a route of `crate::routes`.** `routes::ROUTES` lists every
  route, `routes::Page::of` names the page an address holds, and
  `routes::get` draws it in the shell for a signed-in operator
  (`crate::shell::shell`), the sign-in or About for a visitor. The page reads
  the gateway through `crate::read` before it draws, and draws a refusal or
  a fault in words, never an empty view.
- **The request's visit carries what the server read of it**
  (`crate::visit::Visit`): the session, the operator, the preference
  cookies, the locale, the address and the console's settings, kept for the
  request's task by the `visit::scoped` layer. A page names its title and,
  for a refusal or a fault of its own, its status as it draws
  (`crate::app::title`, `crate::pages::set_status`).
- **Markup the console writes is escaped where it is written.** A page
  writes its own markup with `crate::pages::h` from parts escaped with
  `crate::pages::t` (FerroUI's `Html::text`) beside the library's
  components; raw text in `h` is an injection. Every value the gateway or a
  replica answers has a hostile-value test (`tests/it`, `escaped`).
- **An action is a plain form that posts to the page it sits on**
  (`crate::pages::here_with_query`), and `routes::post` draws that page
  again with the answer and the fields as posted (FerroUI `docs/consoles.md`,
  An action that answers on its page): the clear on Integrity, the drift
  repair on a stored query's page, a query on the query console, and every
  configuration form on its configuration page. `crate::act::fields::Fields`
  reads the body as posted: a field posted more than once is named with
  `[]`, and a row's field in one dotted step (`form[query.0.url]`). A
  download (the self-description, a revision's TOML) is a plain route.
- **Rows are FerroUI's F7 rows** (`view::repeat`): a sub-card per endpoint,
  setting or new secret (`Fields::Card`), a row of one field per profile or
  list value (`Fields::One`). Add and Remove are submit buttons with a name
  and a value (`add-parameter`, `form[edit]`); with the modules refused the
  page answers with the form drawn again from what was posted
  (`crate::configure::draft`) and runs nothing, and with them FerroUI's
  `repeat` module adds a copy of the blank row under a fresh key (`new-1`)
  or takes a row out in place, and the form posts the rows it holds. A form
  whose action was refused, or that a replica refused, is drawn again as
  posted, the refused field marked with its line and `aria-invalid`
  (`configure::render::parts::drawing`). A secret is never drawn again.
- **A confirmation is a native modal dialog.** `popover_form` and the
  console's `confirmed` part hold the reason and post `confirm`; an action
  without it asks no replica. A configuration table's removal is FerroUI's
  `danger_form_dialog`, whose form posts to the page it sits on. Each dialog
  id is unique on its page: a table row's removal takes an id made from its
  row (`plan::removal_dialog`), and the item's own page keeps a removal of
  its own. An explanation, such as How configuration works, is FerroUI's
  `info_dialog`.
- **A secret's value field is FerroUI's masked input** (R10) with the
  hold-to-show eye, never filled, and `view::form::secret_mark` beside it.
- **The commit-confirm countdown is FerroUI's F2** (`view::countdown`): the
  server draws the seconds left and the trial's length, and the `countdown`
  module counts down from there.
- **Return paths are checked.** A preference post returns to the page it
  names only when that is a plain path of the console
  (`prefs::return_path`).
- **The query console's address keeps no key it does not read.** A `GET`
  of `/query` with any key but `stored` and `version` is sent to the same
  page without it (`server::clean_query_address`), so a parameter a link
  put in the address is never echoed into the fields that name the page.

## 2. The server half

- A page's read is thin: check the session, call `crate::gateway`, map the
  answer to the page's types. Errors are types (`read::Refusal`,
  `read::Fault`), and every failed gateway call is logged once by
  `read::stopped` with what it asked for, never a body, a token or a query.
- A refusal shows its status and stable code (FerroUI's refusal R18); the
  console's own refusal of input it cannot send is an alert notice.
- The security headers (`server::router`): the family's `nosniff`, `DENY`
  framing and `no-referrer` from `ferrohealth_server::console`, and the
  Content-Security-Policy of a server-rendered console
  (`ConsoleHeaders::server_rendered`, ferrohealth-server 0.4.2), `script-src
  'self'; style-src 'self'` with no nonce, `frame-ancestors 'none'`, and a
  `form-action` naming the provider's end-session origin. The body limit
  holds before anything reads a body (`tests/it/body_limit.rs`). Nothing is
  compressed, so a page carrying what the operator entered opens no
  compression side channel (BREACH).
- FerroUI's assets are served by the library's own router at
  `ferrohealth_ui::serve::PREFIX`, under the path of its version with a
  year's immutable cache; the console serves no stylesheet, theme or script
  of its own.

## 3. Accessibility

The bar is FerroUI's: WCAG 2.2 Level AA in the light and the dark theme,
keyboard operable, no meaning on colour alone, reflow at 320 CSS px. The
console serves FerroUI's stylesheet and its FerroFED theme exactly as the
crate ships them (`tests/it/server.rs`), so FerroUI's contrast checks hold
for every page; the console keeps no stylesheet of its own.

## 4. Testing and gates

- Business logic lives outside the page code, in plain types with unit
  tests.
- The HTTP surface is tested in-process: `crate::server::router` driven with
  `tower::ServiceExt::oneshot`, the gateway and the admin listeners as
  `wiremock` stubs answering the specification's own examples
  (`tests/it/stub.rs`). Every page has a test that it answers whole under
  the strict policy (`tests/it/support.rs`, `strict`), needs a session and
  escapes what the gateway names; an action's test posts to the page as the
  console's own form does (`support::answer_post`).
- Gates for every console change (`/ui-gates`): `cargo fmt`; clippy with
  `--all-targets` at `-D warnings`; `cargo nextest run -p ferrofed-console`;
  the architecture tests of `ferrofed-engine`.
- **Browser journeys** drive the console in headless Chrome: Rust only,
  `thirtyfour` over chromedriver, in the testkit's `tests/it/journeys/`
  behind `FERROFED_JOURNEYS=1`. A journey fails on any error the browser
  logs, waits on elements, never on a sleep, and waits on FerroUI's loader
  mark (`browser::scripted`, `ferrohealth_ui::script::MARK`) and every
  element's ready mark (`ferrohealth_ui::script::READY`) before it
  touches a module's behaviour. The journeys run with the modules loaded
  and refused (`views::every_page_reads_whole_with_the_modules_refused`
  withholds every module, as FerroUI's gallery does), and every page fits a
  window 320 px wide (`reflow`). A new screen gets its journey in the same
  change.
- **Never weaken a gate to make a change pass.**

## 5. Icons

`ferrohealth_ui::icon::Glyph`, each beside the word it illustrates, and no
icon crate of the console's own (FerroUI §9). A glyph the console needs and
the library lacks is a FerroUI issue.

## 6. Strings and locales

No string the console draws lives outside a catalog (FerroUI §13).

- The console's catalog is one file per area under
  `app/ferrofed-console/locales/<tag>/`, embedded by
  `fluent_templates::static_loader!` in `crate::locale`, each message one
  `Msg` constant in `src/locale/<area>.rs`. The loader registers FerroUI's
  number formatter, and `common.ftl` supplies `number-group` and
  `number-decimal`. A test holds every catalog directory to the full message
  set, and a throwaway catalog under `tests/locales/` shows the words change
  with no code change.
- The server chooses the locale once per request
  (`crate::locale::requested_from`): the operator's language preference,
  else the best language of `Accept-Language` the catalog holds, else
  `[server] locale`. The visit carries it; `crate::locale::tr` looks the
  console's words up in it, every FerroUI component is handed it
  (`crate::locale::current`), and the document writes it as `<html lang>`.
- A handler's plain-text answer is a catalog message too, in the
  request's language (`server::plain`).
- `scripts/checks/stray-literals.sh` runs in CI with an empty baseline:
  every string a reader sees is in the catalog. It passes over the lines
  that speak to a developer or the operator's terminal (a log macro, a
  `println!` or `eprintln!` with every line of its arguments, the
  configuration's refusals and the command line's help, which stay in
  English as the log does), and each literal no reader sees as words is
  allowed with its reason in `scripts/checks/stray-literals.allow`. A
  rise over `scripts/checks/stray-literals.baseline` fails; a fall passes
  with a note, and a pull request never runs `--update`: the daily
  `ratchet-tighten.yml` lowers the baseline on `main` (#1467).
- The pseudo-locale `qps` (`crate::locale::PSEUDO_TAG`) draws every word
  of the console and of FerroUI accented and two fifths longer, as
  FerroUI's gallery does. It ships no catalog directory and no request can
  choose it; `[server] locale = "qps"` serves it to a browser that asks for
  no shipped language. The `journeys::pseudo` journey holds every page to
  fitting a window 1440 and 390 px wide in it, and with
  `FERROFED_SCREENSHOTS=1` captures each under the ignored
  `target/console-screenshots/pseudo`, to be read after a layout change.
