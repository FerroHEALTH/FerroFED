- The conformance statement (#95, #89): a book page under Evaluate that claims
  the Federation-Gateway profile of the Federation Tier with AQL 0.9.0 at
  commit `7162d0c`, with every §17 point and §16.3 track and how each is
  scored, every deferral with its actor and reason, and the FerroEHR and
  EHRbase images the tests ran against. Its generated part and the new page of
  marked tests come from `scripts/conformance/matrix.sh --statement-write`,
  and the docs build and the conformance-matrix guard fail when either
  disagrees with the matrix. The guard now refuses a `planned` point or
  track, so a re-pin scores or defers every point it adds.
