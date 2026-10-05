---
name: no-personal-data-in-requests
description: "No worker sends the owner's email address or any personal contact to an external service, including a crates.io or API User-Agent header; use a neutral agent string"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

On 2026-10-03 a worker checking crate versions sent one crates.io API request
whose User-Agent carried the owner's email address. crates.io's guidance asks
for contact details in a User-Agent, which is what the worker followed. The
session rule says the owner's address identifies the owner only and is never
sent to an unrelated service.

**Why:** the address is personal data, and a request header to a public API
gets stored in that service's logs outside the owner's control.

**How to apply:** every external call (crates.io, GitHub raw content, a spec
site, a package registry) uses a neutral User-Agent such as
`ferrofed-pin-check` and carries no personal contact. Every worker brief that
may query an external API says so. A slip is reported to the owner, never
hidden.
