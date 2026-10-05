---
name: domain-ferrofed-eu
description: "The public domain is ferrofed.eu (the README and the repository homepage name it); on the family model it is a GitHub Pages setting, never a CNAME file in the tree"
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The README says "Its site will be <https://ferrofed.eu/>", and the GitHub
repository homepage is set to it. The family serves `ferroterm.eu` and
`ferrobridge.eu` the same way: no `CNAME` file in the tree, the domain is a
Pages repository setting (verified for the account, HTTPS enforced, source
GitHub Actions), and the repository carries it in the manifest `homepage`,
`CITATION.cff`, the landing page's canonical and Open Graph URLs, and any
shields.io endpoint badge host.

**How to apply:** the site itself and the Pages setting are v0.0.1 issues;
the setting is an owner action. Never add a `CNAME` file.
