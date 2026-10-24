<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The operator console

The operator console is a web interface to a FerroFED federation, for the
people who run it. It ships as its own binary, `ferrofed-console`, and its own
image, `ghcr.io/ferrohealth/ferrofed-console`, beside the gateway. No
specification governs the console: it is FerroFED's own design.

The console was named `ferrofed-viewer` before v0.0.12. To upgrade, pull
`ghcr.io/ferrohealth/ferrofed-console` in place of
`ghcr.io/ferrohealth/ferrofed-viewer`, rename every `FERROFED_VIEWER__`
override to `FERROFED_CONSOLE__` and `FERROFED_VIEWER_CONFIG` to
`FERROFED_CONSOLE_CONFIG`, and run the binary as `ferrofed-console`. The
cookies are renamed with it, so every operator signs in once more.

The console is one more client of the gateway. It reaches the gateway over
HTTP, on the same public surface any other client uses, and it adds no
behaviour to the gateway. It stores no clinical data: the query console
shows the signed-in operator the rows a query answered, and neither the
console nor the browser keeps them. Every page and every answer is
`Cache-Control: no-store`, and the page puts nothing in browser storage, a
service worker or a URL.

## What is built

- **A sign-in page and a shell, built on FerroUI.** The console draws every
  page with `ferrohealth-ui`, the FerroHEALTH component library, and its
  FerroFED theme. The server renders each page whole, and the browser
  hydrates only the few interactive parts (islands), so every page reads and
  every action posts with no WebAssembly. A visitor reads a sign-in card. A
  signed-in operator lands on a dashboard and reads every page in a shell: a
  top bar with the find field, the gateway's status (which names the
  gateway's version and the replica that answered), and the operator's
  name, which opens a menu holding Your preferences, the theme and Sign out;
  and a navigation grouped under Overview, Federation, Queries, Gateway and
  Console that marks the page you are on and folds to a rail. On a narrow
  screen the navigation is a drawer the Menu button opens. Your preferences
  (`/preferences`) has three tabs, Appearance, Language and region, and
  Keyboard; each preference is kept in an `HttpOnly` cookie of its own
  (`ferrofed_console_theme`, `ferrofed_console_density`,
  `ferrofed_console_motion` and the others), holding one word and nothing
  else, for a year, and the server draws each page in them. The footer of
  every page names the product, its version and Cadasto B.V., and links the
  About page, which names the manufacturer in full with the gateway the
  console reads and holds the gateway's `OPTIONS` document, with its
  download.
- **Operator sign-in at an OpenID Provider:** `GET /login` redirects the
  browser to the provider's authorization endpoint with the authorization
  code grant, a `nonce` and a PKCE challenge (RFC 6749 §4.1, RFC 7636). The
  `state`, the `nonce` and the PKCE verifier stay on the console's server as
  a pending sign-in, which the browser knows only by an opaque `HttpOnly`
  cookie that expires with it. The provider's redirect back to
  `/auth/callback` is checked against it once. The code then goes to the
  provider's token endpoint with the PKCE verifier (RFC 6749 §4.1.3, RFC 7636
  §4.5), with the client secret as HTTP Basic for a confidential client. The
  ID Token that comes back must verify against the provider's JWK Set and
  carry the configured issuer, the console's client id as its one audience,
  an `exp` still ahead, and the sign-in's `nonce` (OpenID Connect Core 1.0
  §3.1.3.7). An ID Token for more than one audience, or signed with a shared
  secret, is refused. Only then does a signed-in session begin, holding the
  operator's access token on the server, and the browser goes back to `/`. A
  session the browser already held ends when the new one begins. An ID Token that fails a check is
  `401`, and a provider that refuses or cannot be reached is `502`.
- **The access token is refreshed on the console's server.** When the
  provider answers the sign-in with a refresh token, the session keeps it
  on the server beside the access token; it never reaches a cookie, a page,
  the browser or a log line (RFC 6749 §10.4). A request that arrives
  within 60 seconds of the access token's expiry, or in the second half of
  a lifetime shorter than two minutes, first sends the
  refresh token to the token endpoint (RFC 6749 §6), with the client secret
  as HTTP Basic for a confidential client, and the session goes on with the
  new access token. A rotated refresh token replaces the old one, which is
  never sent again, and one refresh of a session runs at a time, so
  requests that arrive together spend a rotated token once. A refresh that
  fails for any reason, an `invalid_grant` refusal, any other refusal, or a
  provider that cannot be reached, ends the session: a view sends the
  browser to `/login` and drops the session cookie. A session with no
  refresh token ends when its access token expires, and one whose provider
  sends no `expires_in` is never refreshed. The ID Token of the sign-in
  stays the sign-out's hint; one a refresh answers is not read. Set
  `offline_access = true` in `[oidc]` to ask for the `offline_access` scope
  and the `prompt=consent` it requires (OpenID Connect Core 1.0 §11), for a
  provider that issues a refresh token only then.
- **Operator sign-out:** the "Sign out" button in the operator's menu,
  which the operator's name in the top bar opens, posts to
  `POST /logout`, which ends the signed-in session on the console's server
  and removes the session cookie. Where `[oidc]` names the provider's
  `end_session_endpoint`, the console then redirects the browser there with
  the session's ID Token as `id_token_hint`, its `client_id`, and the
  `post_logout_redirect_uri` you registered with the provider (OpenID
  Connect RP-Initiated Logout 1.0 §2); without one the browser goes back to
  `/`. A `GET /logout` is `405`.
- **Requests from the console's own pages only.** Every request that is not
  a `GET`, `HEAD` or `OPTIONS`, the sign-out and every form among them, must
  come from the console's own pages: a request the browser marks
  `Sec-Fetch-Site: same-origin`, or, without fetch metadata, one whose
  `Origin`, or else whose `Referer`, has the origin of `redirect_uri`. Any
  other, one that names no origin among them, is `403` before the session
  is read or the gateway asked, so no other site can sign an operator out or
  spend their sign-in on a query. The session cookie is `SameSite=Lax` as
  well, and every action is a `POST` to the page it sits on: a `GET` runs
  nothing.
- **Two separate pools of server-side state.** Pending sign-ins live for
  `sign_in_timeout_s` and are bounded by `max_sign_ins`; a full pool drops
  its oldest pending sign-in, so a flood of `GET /login` holds at most that
  many and stops costing anything once it ends. A signed-in session is
  created only once a sign-in completes, lives for `idle_timeout_s` without
  a request and `absolute_timeout_s` at most, and is bounded by
  `max_sessions`, which refuses a new session rather than evicting one. The
  idle and the absolute timeouts end a session whatever its tokens say,
  however long a refresh token would last. No
  sign-in traffic ever removes a signed-in session. The console does not
  limit sign-ins per client: behind a load balancer it cannot tell clients
  apart without trusting a forwarded address, so put a rate limit for
  `/login` at the edge. Both pools live in the console's memory: a restart
  ends every session, and more than one replica needs a load balancer that
  keeps an operator on one replica.
- **The pages**, each rendered on the console's server from the gateway's
  own surface, called with the signed-in operator's own access token:

  | Page | What it shows | Read from |
  |---|---|---|
  | `/` | the dashboard: the organisations, the nodes reachable, the open incidents and the running configuration, the nodes that need attention, the topology in brief, the open incidents and the configuration at a glance | the pages below |
  | `/topology` | the gateway, the services it asks and the organisations with their nodes, as a map or a list, filtered by state, with How it is drawn saying what each edge's line and mark mean | `OPTIONS {base}/` and `GET {base}/operator/dependencies` |
  | `/organisations`, `/organisations/nodes` | every organisation with its nodes and worst state, and every node with its organisation, `system_id`, state and median latency | the same |
  | `/organisations/{org}`, `/organisations/{org}/{node}` | one organisation and its nodes; one node with its facts, its endpoints and, on its Routes tab, the creating systems the routing table sends to it | the same, and `GET {base}/operator/creating-systems` |
  | `/services` | every service the gateway asks, grouped, with its last observed state | `GET {base}/operator/dependencies` |
  | `/integrity` | the open and the resolved integrity incidents and the count of each kind since the gateway started | `GET {base}/operator/incidents` |
  | `/stored-queries`, `/stored-queries/{name}` | every stored query by namespace with its versions, a namespace the gateway takes no write under marked read-only; one stored query with the AQL of the version shown and, on its Versions tab, every version | `GET {base}/operator/stored-queries` and `/operator/stored-queries/{name}` |
  | `/self-description` | what `OPTIONS {base}/` declares, on the tabs Federation (with the manufacturer it names), Capabilities, ITS-REST areas, Limits and Endpoints | `OPTIONS {base}/` |
  | `/search` | the organisations, nodes and stored queries the find field names | the same |

  A page without a signed-in session sends the browser to `/login` before
  the gateway is asked anything, whatever the case of its path or a
  trailing slash. A page the gateway refuses shows the gateway's status and
  its stable error code; a `401` links back to sign-in. A page whose answer
  the console cannot read says so with the status, and one the gateway
  cannot answer says so too; no page is ever silently empty. The
  `/operator/` routes need a token carrying the
  [operator scope](authentication.md#the-operator-surface) its issuer names.
  No page shows a patient identifier, a token or clinical data.

  The top bar's status names the gateway replica that answered, as the
  gateway's [operator surface](authentication.md#the-operator-surface) names
  it. The health, the incidents and the routing table are each replica's
  own, so read them as that replica's view, never the federation's. The
  console reads the gateway through the one `base_url` it is configured
  with, so behind a load balancer it shows the replica the balancer chose
  for each request.
- **The query console, `/query`:** an AQL query, or a stored query by name
  and optional version, runs through the gateway's own query surface,
  `POST {base}/v1/query/aql` or `POST {base}/v1/query/{name}`, as the
  signed-in operator, exactly as any client sends it. The form offers what
  the gateway's `OPTIONS {base}/` declares: the member endpoints and
  organisations to target (`openEHR-federation-endpoint`,
  `openEHR-federation-organisation`, §8.4), its dedup modes
  (`openEHR-federation-dedup`, §10), and the best-effort opt-in
  (`openEHR-federation-completeness: partial`, §11.4) where it offers one.
  `offset` and `fetch` are optional. The answer shows the rows under
  `columns[]` as the gateway renders them, and every endpoint of
  `meta.federation.endpoints[]` with its status, latency, row count and
  error. An answer whose `meta.federation.complete` is `false` says
  "Incomplete answer" in words before its rows. A `504` or `424` the gateway
  answers under its all-or-nothing default shows its status and the
  endpoints that failed, from the diagnostic envelope it carries (§11.4). A
  refusal shows its status and stable error code, and a result set with no
  readable `meta.federation` is shown as unreadable, never as an answer.

  Name a patient through a parameter, a name and a value on a row of their
  own, never in the AQL text; each value is sent as a string. The form posts
  its fields in the request body to the console, which sends them to the
  gateway in the request body and answers with the page, the answer under
  the form, so nothing you enter reaches a URL or the browser history.
  The fields carry `autocomplete="off"`, and the console logs a refusal by
  its status and code alone, never the query or a parameter.
- **The operator's admin actions**, where `[admin]` names at least one
  gateway replica. `/integrity` offers the clear of a held `ehr_id`, which
  calls `POST /admin/ehr-ids/{ehr_id}/clear` on the admin listener of the
  replica you pick, or of every replica one after another, since each
  replica keeps its own routing state. The form offers the `ehr_id`s the
  incidents name. A stored query's page offers the drift repair of the
  version shown, which calls
  `POST /admin/stored-queries/{name}/{version}/distribute` with the members
  in `openEHR-federation-endpoint` (`*` for every active member) on the
  first replica configured, since the replicas share the stored-query store.
  Each action is a form in a confirmation dialog, which the console checks
  again on its server. Its answer shows at the top of the page and names the
  replica it went to: what a clear found there, or whether every member took
  the version and each member's outcome. A replica that refuses shows its
  status and stable error code, a `401` links back to sign-in, and one the
  console cannot reach is named as such. The console calls the admin
  listener with your own access token, which never reaches the browser, so
  the gateway admits the action only for a token carrying
  `auth.product_scope` and records you as the operator who ran it.
- **`/configuration`**, where `[admin]` names at least one gateway replica
  whose configuration is managed (`config.ui = "managed"`). It reads the
  first replica's configuration revisions. The header names the revision
  that runs and the one saved and not applied, and a card above every tab
  shows what the saved revision changes, each setting with its running
  value marked `−` and its new value marked `+`, and its apply. Five tabs,
  each at its own path, hold the rest: `/configuration/members`, every
  member with its owner and state; `/configuration/credentials`, every
  onward credential and binding section with its secrets as set, with the
  file each is read from, never by value; `/configuration/stored-queries`,
  the stored versions by name; `/configuration/history`, the newest
  revisions, each with its download as TOML and its comparison with the
  running revision, filtered by origin in the Origin menu; and `/configuration/import-export`, the running
  revision's download and the import of TOML as a dry-run revision. A
  member the registry document or a care services directory provides opens
  as a read-only detail that names its source. A console member is added
  or edited on a page of its own (`/configuration/members/new`,
  `/configuration/members/{node_id}`), each Query API and FHIR endpoint a
  sub-card of its own fields, with its categories as checkboxes and its
  profiles as a list of rows, and saved as a new revision, which you then apply. A
  credential's form shows the fields of the grant you choose, and each
  secret is kept, set by its value in a masked field the page never
  fills, whose eye shows what you typed while you hold it, read from a file, or cleared; a binding section's form shows each
  setting as a field of its own. When the running revision awaits its
  confirmation, the card counts down to its deadline and offers Confirm and
  Roll back; when the saved revision takes a second operator's approval, it
  shows who made it, who approved it, and Approve
  ([Credentials and binding sections](configuration-revisions.md#credentials-and-binding-sections)).
  An apply, a roll back, a confirmation, an approval, a removal and a
  stored version each ask you to confirm in a dialog of their own, which the
  console checks again; a save is not applied and asks none. A console member's or
  section's row menu (⋯) holds Edit and Remove…, which asks in a dialog for
  the reason the revision keeps; its own page offers the removal too.
  How configuration works opens an explanation of what stays in the file
  and what a revision holds. Every form is a plain post to its page: with
  no WebAssembly, Add and Remove of a row post the form and answer with it
  one row longer or shorter, and with it they add or remove the row in
  place; a form's answer shows at the top of the page, and a
  form the console or a replica refuses is drawn again as you typed it, the
  field the console refuses marked with what is wrong under it. A secret you
  typed is never drawn again. A replica whose configuration is not managed answers
  `404`, and the page says so and offers no form
  ([Console-managed resources](configuration-revisions.md#console-managed-resources)).
- **`GET /health`**, which answers `200` while the process serves, and the
  `healthcheck` command the image runs against it.

Every answer carries a Content-Security-Policy whose script source is a nonce
minted for that answer, `X-Content-Type-Options: nosniff`,
`X-Frame-Options: DENY` and `Referrer-Policy: no-referrer`, and no page is
stored by a cache. The site bundle under `/pkg/` is served brotli- or
gzip-compressed, as the browser's `Accept-Encoding` chooses; pages are
never compressed, so no compression side channel reads what an operator
entered.

The console serves plain HTTP and sends no `Strict-Transport-Security`
header. Terminate TLS at a reverse proxy in front of it and set
`Strict-Transport-Security` (RFC 6797) there, as the
[hardening guide](hardening.md#tls) asks: a browser that has seen the header
then reaches the console over HTTPS alone, so no one on the network path can
turn the operator's sign-in into a plain HTTP request.

The query console's answer is rendered on the console's server, with every
value a node sent escaped, and it works the same with no WebAssembly: Run is
a plain form post.

## The screens

Each screen is shown in the light theme, with a link to the same screen in
the dark theme. The testkit takes them against a development gateway over
two stub nodes, node A that answers and node B that fails, so every page
carries the amber development banner. To take them again, build the site
bundle, install the pinned browser with `scripts/release/journey-tools.sh
target/journey-tools`, and run the capture behind its gate:

```sh
scripts/release/console-site.sh --release
FERROFED_JOURNEYS=1 FERROFED_SCREENSHOTS=1 \
  cargo nextest run -p ferrofed-testkit -E 'test(/screenshots::/)'
```

A console route with no screenshot fails the testkit's
`every_console_route_has_a_screenshot_in_light_and_dark`.

### Sign-in

![The sign-in card: the FerroFED Console mark, "Sign in", the button that signs in with your organisation's account, and the line that the console holds no clinical data.](img/console/sign-in.png)

[The sign-in card in the dark theme](img/console/sign-in-dark.png)

### Dashboard

![The dashboard: four counters for the organisations, the nodes reachable, the open incidents and the running revision, over the nodes that need attention, the topology in brief, the open incidents and the configuration at a glance.](img/console/dashboard.png)

[The dashboard in the dark theme](img/console/dashboard-dark.png)

### Topology

![The topology: the gateway, its services and the organisations with their nodes, each with its state, and the filter by state.](img/console/topology.png)

[The topology in the dark theme](img/console/topology-dark.png)

### Organisations

![The organisations: each organisation with its nodes, endpoints and worst node state.](img/console/organisations.png)

[The organisations in the dark theme](img/console/organisations-dark.png)

![Every node across the organisations, with its organisation, system_id, state and median latency.](img/console/nodes.png)

[Every node across the organisations, with its organisation, system_id, state and median latency in the dark theme](img/console/nodes-dark.png)

![The page of organisation org-a: its nodes and their states.](img/console/organisation.png)

[The page of organisation org-a in the dark theme](img/console/organisation-dark.png)

![The page of node node-a: its facts and the state of each endpoint, with its Endpoints and Routes tabs.](img/console/node.png)

[The page of node node-a in the dark theme](img/console/node-dark.png)

![The Routes tab of node node-a: its own system_id and the 99 creating systems the routing table registers for it.](img/console/node-routes.png)

[The Routes tab of node node-a in the dark theme](img/console/node-routes-dark.png)

### Services

![The services the gateway asks, grouped, with the state of each.](img/console/services.png)

[The services the gateway asks, grouped, with the state of each in the dark theme](img/console/services-dark.png)

![The page of the identity cross-reference service.](img/console/service.png)

[The page of the identity cross-reference service in the dark theme](img/console/service-dark.png)

### Integrity

![The integrity page: the Open and Resolved tabs, the open incidents, the count of each kind since the gateway started, and the Clear an EHR id button.](img/console/integrity.png)

[The integrity page in the dark theme](img/console/integrity-dark.png)

### Query console

![The empty query console: the AQL and Stored query tabs, the AQL editor, a parameter row, offset and fetch, and the federation options, above the Run button.](img/console/query.png)

[The empty query console in the dark theme](img/console/query-dark.png)

![The Stored query tab of the query console: the name and the version of the stored query to run.](img/console/query-stored.png)

[The Stored query tab of the query console in the dark theme](img/console/query-stored-dark.png)

![A complete answer: the fan-out reads Complete, node-a-pub answered and node-b-pub excluded, over the row node A sent and the raw answer.](img/console/query-complete.png)

[A complete answer in the dark theme](img/console/query-complete-dark.png)

![An incomplete answer: the notice that node-b-pub did not answer, the fan-out with node-b-pub in node-error, and the row node A sent.](img/console/query-partial.png)

[An incomplete answer in the dark theme](img/console/query-partial-dark.png)

![A refused query: the gateway's 400 and its code not-aql.](img/console/query-refused.png)

[A refused query in the dark theme](img/console/query-refused-dark.png)

### Stored queries

![The stored queries by namespace: the read-only eu.ferrofed.eehrxf namespace with its patient summary queries, and the org.example namespace, each query with its versions.](img/console/stored-queries.png)

[The stored queries by namespace in the dark theme](img/console/stored-queries-dark.png)

![The page of org.example::journey_001: the highlighted AQL of version 1.0.1, the Run in the query console button and the Send again repair.](img/console/stored-query.png)

[The page of org.example in the dark theme](img/console/stored-query-dark.png)

![The Versions tab of org.example::journey_001: each version with when it was saved.](img/console/stored-query-versions.png)

[The Versions tab of org.example in the dark theme](img/console/stored-query-versions-dark.png)

### Self-description

![The Federation tab of the self-description: the federation id, the specification, the key set, the manufacturer and the replica that answered.](img/console/self-description.png)

[The Federation tab of the self-description in the dark theme](img/console/self-description-dark.png)

![The Limits tab of the self-description.](img/console/self-description-limits.png)

[The Limits tab of the self-description in the dark theme](img/console/self-description-limits-dark.png)

### Configuration page

The page opens on the deployment-wide settings, one tab per section:
Federation behaviour, Fan-out and paging, Consent, Stored queries, Access
log, FHIR face and Authentication. Authentication is read-only, because only
the configuration file sets it: the audience, the operator scope, the token
issuers and each trusted issuer, with every claim name, the correlation
header and the added countries of a national contact point it declares. Each setting is a row with its value, where the value
comes from (the configuration file, an environment override, the running
revision, or the default), its configuration key, and a Next start mark
when a change takes effect only at the next start of each replica, with
the value that start takes once a saved change waits for it. A secret is
named by the file it is read from, never by its value. The rows come from
the gateway's running configuration (`GET /admin/config/running`,
[Configuration revisions](configuration-revisions.md#the-running-configuration)).

Edit on a tab opens a form with a control for each setting of one value;
saving it posts the settings you changed, and only those, as a revision,
which you then apply as any revision. A list or a table, such as the
category map, and a secret are changed by an import. With configuration
revisions off (`config.ui = "off"`), the six settings tabs show alone and
read-only.

![The Federation behaviour tab: each setting with its value, where it comes from and its Next start mark, under the card of what the saved revision 2 changes, with the tab's Edit.](img/console/configuration-federation.png)

[The Federation behaviour tab in the dark theme](img/console/configuration-federation-dark.png)

![The edit of the Federation behaviour tab: a field or a choice for each setting, a reason, and the Save as revision 3 button.](img/console/configuration-settings-edit.png)

[The edit of the Federation behaviour tab in the dark theme](img/console/configuration-settings-edit-dark.png)

![The Stored queries tab: the store's backend and its connection, named by the file it is read from, above the stored versions.](img/console/configuration-stored-queries.png)

[The Stored queries tab in the dark theme](img/console/configuration-stored-queries-dark.png)

![The Members tab of the configuration page: revision 1 running and revision 2 saved with How configuration works, what revision 2 changes with its Apply revision 2 button, and the members with their owners and states, the console member's row with its row menu.](img/console/configuration-members.png)

[The Members tab of the configuration page in the dark theme](img/console/configuration-members-dark.png)

![The Credentials and bindings tab: the console's credentials of node-a-pub, running, with its password set and read from a file.](img/console/configuration-credentials.png)

[The Credentials and bindings tab in the dark theme](img/console/configuration-credentials-dark.png)

![The History tab: the Origin menu, and each revision with its origin, its author, its reason and its download as TOML.](img/console/configuration-history.png)

[The History tab in the dark theme](img/console/configuration-history-dark.png)

![The member form: the organisation, the node, one Query API endpoint sub-card with its Remove, the Add endpoint and Add FHIR endpoint buttons, a reason, and the Save as revision 3 button.](img/console/configuration-member-form.png)

[The member form in the dark theme](img/console/configuration-member-form-dark.png)

### Your preferences

![The Appearance tab of Your preferences: the theme and the density as cards, and reduced motion.](img/console/preferences.png)

[The Appearance tab of Your preferences in the dark theme](img/console/preferences-dark.png)

![The Language and region tab of Your preferences.](img/console/preferences-language.png)

[The Language and region tab of Your preferences in the dark theme](img/console/preferences-language-dark.png)

### Find

![The answer of the find field for "node": the organisations and nodes it names.](img/console/search.png)

[The answer of the find field for "node" in the dark theme](img/console/search-dark.png)

### About and not found

![The About page: the versions of the console and the gateway, the manufacturer Cadasto B.V. with its postal address, contact and website, and the gateway's OPTIONS document with its download.](img/console/about.png)

[The About page in the dark theme](img/console/about-dark.png)

![The not-found page: "Not found", the path the console has no page for, and the way back to the dashboard.](img/console/not-found.png)

[The not-found page in the dark theme](img/console/not-found-dark.png)

## Why a console of its own

The gateway authenticates every request by bearer token and holds no browser
session ([Client authentication](authentication.md)). The console holds the
operator's session on its own server and calls the gateway with the
operator's token, so the gateway's authentication stays as it is. The gateway
also serves no static files: it answers `501` on a path it does not serve,
and its request log carries no body and no header value.

## Configuration

The console reads a TOML file named by `--config` or `FERROFED_CONSOLE_CONFIG`.
Every key can be set from the environment as
`FERROFED_CONSOLE__<SECTION>__<KEY>`, and the client secret can be read from a
file through `client_secret_file`. A refused configuration names the key and
the line, never a value, so no secret reaches a message or a log.

```toml
[server]
listen = "0.0.0.0:3000"
site_root = "/site"

[gateway]
base_url = "https://gateway.example.org/"
timeout_ms = 30000

[session]
secure_cookie = true
sign_in_timeout_s = 300
max_sign_ins = 1000
idle_timeout_s = 1800
absolute_timeout_s = 43200
max_sessions = 10000

[oidc]
issuer = "https://idp.example.org/realms/ferrofed"
authorization_endpoint = "https://idp.example.org/realms/ferrofed/protocol/openid-connect/auth"
token_endpoint = "https://idp.example.org/realms/ferrofed/protocol/openid-connect/token"
jwks_uri = "https://idp.example.org/realms/ferrofed/protocol/openid-connect/certs"
client_id = "ferrofed-console"
client_secret_file = "/run/secrets/console-client-secret"
redirect_uri = "https://console.example.org/auth/callback"
scopes = ["openid"]
offline_access = false
end_session_endpoint = "https://idp.example.org/realms/ferrofed/protocol/openid-connect/logout"
post_logout_redirect_uri = "https://console.example.org/"

[[admin.replicas]]
name = "gateway-0"
admin_url = "https://ferrofed-0.ferrofed-admin.internal:9464/"

[[admin.replicas]]
name = "gateway-1"
admin_url = "https://ferrofed-1.ferrofed-admin.internal:9464/"
```

`[server] locale`, `en` by default, is the language a page renders in when
neither the operator's language preference nor the browser's
`Accept-Language` names one the console ships; English is the one it ships
today, and a language joins as a catalog directory with no code change.
`locale = "qps"` is the pseudo-locale, which draws every word accented and
two fifths longer to show where a longer translation would not fit; it is
for checking the layout, never offered to an operator.

With `secure_cookie = true` the session and sign-in cookies carry the
`__Host-` prefix, which binds each to the console's host (RFC 6265bis
§4.1.3.2); with it off, for a loopback trial, they carry none.

`secure_cookie = false` sends the cookies over plain HTTP, so the console
refuses it at start unless `redirect_uri` is an `http` URL on a loopback
host (`localhost`, `127.0.0.0/8` or `::1`). The refusal names
`session.secure_cookie`. A console without `[oidc]` sets no cookie, so the
key has nothing to govern there and is not checked.

Without an `[oidc]` table the console offers no sign-in, and `GET /login`
answers `503`. The provider's URLs must be `https` unless their host is
loopback, and `scopes` must include `openid`. `offline_access`, off by
default, adds the `offline_access` scope and `prompt=consent` to the
sign-in; `scopes` must not list `offline_access` itself, and the refusal
names `oidc.scopes`. The console refreshes the access token with whatever
refresh token the provider issues, with or without it. `end_session_endpoint` is
optional, and `post_logout_redirect_uri` needs it. `ferrofed-console config check`
reads and checks a configuration without binding a socket.

`[[admin.replicas]]` names the admin listener of each gateway replica the
console runs the operator's admin actions on. Each entry has a `name`, which
the console shows and the operator picks the replica by (letters, digits,
`.`, `_` and `-`, at most 64, each name once), and an `admin_url`, the
`http` or `https` root of that replica's admin listener. Without an entry
the console offers no admin action, and its views are as before. The list
can be set from the environment as one TOML value, for example
`FERROFED_CONSOLE__ADMIN__REPLICAS='[{ name = "gateway-0", admin_url = "https://ferrofed-0.internal:9464/" }]'`.

The console's server calls each admin listener directly, so it must reach
it on an internal network that nothing outside the deployment can reach;
never publish an admin listener. Its default bind is loopback, so set the
gateway's `[metrics] listen` to a non-loopback internal address with
`allow_remote = true` ([Metrics](metrics.md#who-the-admin-listener-serves)),
and prefer an `https` admin listener, since the operator's access token
travels on each call. The console's client of the admin listener is the one
it calls the gateway with: it trusts the system's root certificates and
presents no client certificate, so an admin listener that asks for mutual
TLS cannot be reached from the console.

The image sets `server.listen` to `0.0.0.0:3000` and `server.site_root` to
the site bundle it carries, runs as the numeric user `65532`, and runs
`healthcheck` as its `HEALTHCHECK`.

## From the release compose file

The `compose.yaml` every release carries runs the console beside the
gateway under the `console` profile, from the published image at the
release's version, with the gateway's hardening: a read-only root
filesystem, every capability dropped, `no-new-privileges`, and CPU and
memory limits. Its port binds the loopback interface, as the gateway's
does.

```sh
# the client secret at your OpenID Provider, read as a file secret
printf '%s' "$CLIENT_SECRET" > secrets/console_oidc_client_secret
export FERROFED_CONSOLE__OIDC__ISSUER=https://idp.example.org/realms/ferrofed
export FERROFED_CONSOLE__OIDC__AUTHORIZATION_ENDPOINT=https://idp.example.org/realms/ferrofed/protocol/openid-connect/auth
export FERROFED_CONSOLE__OIDC__TOKEN_ENDPOINT=https://idp.example.org/realms/ferrofed/protocol/openid-connect/token
export FERROFED_CONSOLE__OIDC__JWKS_URI=https://idp.example.org/realms/ferrofed/protocol/openid-connect/certs
export FERROFED_CONSOLE__OIDC__CLIENT_ID=ferrofed-console
export FERROFED_CONSOLE__OIDC__CLIENT_SECRET_FILE=/run/secrets/console_oidc_client_secret
export FERROFED_CONSOLE__OIDC__REDIRECT_URI=http://127.0.0.1:3000/auth/callback
export FERROFED_CONSOLE__SESSION__SECURE_COOKIE=false
docker compose --profile console up --wait
```

Then open `http://127.0.0.1:3000`. The console reaches the gateway over
the compose network at `http://ferrofed:8080/`
(`FERROFED_CONSOLE__GATEWAY__BASE_URL`). `secure_cookie = false` is
admitted only because the redirect URI is plain HTTP on loopback; behind a
TLS reverse proxy, drop it and name the proxy's URL as the redirect URI.
Without the `FERROFED_CONSOLE__OIDC__*` variables the console starts and
offers no sign-in. Every variable the service passes through is set only
when your shell or `.env` sets it.

The configuration page needs a replica's admin listener. Set the gateway's
`[metrics] listen = "0.0.0.0:9464"` with `allow_remote = true` in
`ferrofed.toml`, leave it unpublished, and name it to the console:
`FERROFED_CONSOLE__ADMIN__REPLICAS='[{ name = "gateway-0", admin_url = "http://ferrofed:9464/" }]'`.
`FERROFED_CONSOLE_PORT`, `FERROFED_CONSOLE_CPUS` and
`FERROFED_CONSOLE_MEMORY` move the port and the limits, and
`FERROFED_CONSOLE_OIDC_CLIENT_SECRET_FILE` another secret file.
