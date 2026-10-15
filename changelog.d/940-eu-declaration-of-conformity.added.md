- The EU declaration of conformity of Regulation (EU) 2025/327 Art 39,
  with every item of Annex IV, written for each release by
  `scripts/release/declaration-of-conformity.sh` from the records at the tag
  and the values only the manufacturer supplies in
  `conformance/eu-declaration-of-conformity.toml`. The release lane attaches
  it as `ferrofed-vX.Y.Z-eu-declaration-of-conformity.md` once that file
  issues it, and refuses a release whose declaration would hold a blank;
  until then a release carries none and says so in a warning (#940).
