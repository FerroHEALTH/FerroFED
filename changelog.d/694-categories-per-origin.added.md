- The access record of a query answered by several endpoints now carries
  the categories of each origin (#694), as Regulation (EU) 2025/327 Annex II
  3.2(c) and (e) ask. Each endpoint's categories are classified from the
  rows it answered with, before the merge, under the same rules as the
  access's own, so an id the map does not hold marks that origin
  `unclassified`. The merged answer, its columns and `DISTINCT` are
  unchanged. An origin's set can name a category of a row the merge then
  cut by `LIMIT` or `DISTINCT`, and never misses one it delivered.
