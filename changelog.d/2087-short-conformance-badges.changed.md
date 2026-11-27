- The Federation Tier, EHDS and CRA badges of the README show their count
  alone, for example `Federation Tier 1.0.1 | 364/383 tested` and
  `EHDS Annex II | 11/18 met`, and link to a new README badges section of
  the book's conformance matrix page, which `scripts/conformance/matrix.sh`
  renders from the same tables: the rest of each count by status and the
  items kept out of it. The badge scripts fail a label past 30 characters or
  a message past 20.
- The MyHealth@EU badges of the README sit under their own heading and
  name the guide and its version alone, for example `Core 1.0.1 | 15/15 valid`
  and `MyHealth@EU catalogue v10.0.0 | 34/46 met`; the book's MyHealth@EU
  page names each badge's build (CI, preview, proof of concept, draft).
