- A received imaging report's performing organization reaches the member's
  structured record. The `eehrxf` 0.6.6 imaging mapping writes an
  `Organization` performer into the Imaging service slot of the examination
  result as a `CLUSTER.organisation.v1`, over FerroBRIDGE's imaging report
  template at v0.0.24. A performer that is no `Organization` is declared to
  the sender as not carried.
