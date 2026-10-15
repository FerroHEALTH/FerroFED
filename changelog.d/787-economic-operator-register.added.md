- The register of economic operators of Regulation (EU) 2025/327 Art 35
  and Regulation (EU) 2024/2847 Art 23: the procedure on the book's
  Complaints and incidents page and in `docs/post-market.md` (what a row
  holds, who enters it and when, how long it is kept, how a market
  surveillance authority's request is answered, and why a public download
  is not a row), the register's public format in
  `docs/registers/economic-operators.tsv`, which holds no row because the
  rows are kept outside the public repository, and the
  `economic-operators` guard in CI. The guard's `--register` mode checks
  the private register's rows and refuses, across its git history, a row
  removed before its retention date or changed after it was entered.
