- The compose quickstart and the end-to-end harness run FerroEHR 5.5.0 and
  FerroPIX 0.0.10, each pinned by digest, and the browser journeys run
  Chrome for Testing 156.0.8078.73. Each node names its configuration file
  in `FERROEHR_CONFIG`, sets its `system_id` as `FERROEHR__REST__SYSTEM_ID`
  and keeps FerroEHR's in-instance subject lookup
  (`[ehr_index] provider = "local"`); FerroPIX reads `public_base_url` at
  the top level. The vendored W3C DID Resolution 1.0 moves to the Candidate
  Recommendation Draft of 2026-10-26 (#1386).
