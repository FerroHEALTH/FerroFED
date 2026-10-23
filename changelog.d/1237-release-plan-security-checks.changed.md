- The release notes end with a link to the CRA information and
  instructions to the user, and the release lane refuses a cut whose
  Security section names an entry without its GitHub security advisory
  (`GHSA-`) or VEX statement, or sits beside other changes without
  "Shipped with functional changes because:" and its reason (Regulation
  (EU) 2024/2847 Annex I Part II(2), (4), (8)). `SECURITY.md` states that a
  security fix ships alone or the release says why, how a fixed
  vulnerability's advisory is published with its release, and the
  `info@cadasto.com` route for a report without a GitHub account.
  `scripts/release/preflight.sh <tag>` runs the release plan's checks
  before the tag (#1237).
