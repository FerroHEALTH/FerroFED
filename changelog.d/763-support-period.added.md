- A support period for every release: five years from its release date, the
  minimum of Regulation (EU) 2024/2847 Art 13(8). `SECURITY.md` lists each
  release with its end date (Art 13(19), Annex II point 7) and says how
  fixes ship in the latest release (Art 13(10)).
  `scripts/release/changelog.sh --assemble` now opens a release's notes
  with its end date, adds its row to `SECURITY.md` and prints the date; a
  pre-release gets no support period.
