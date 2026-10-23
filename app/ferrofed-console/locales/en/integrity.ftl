# Integrity.

integrity-title = Integrity
integrity-subtitle = The incidents the replica keeps, and the counts of each kind since it started.
integrity-open = Open
integrity-resolved = Resolved
integrity-open-count = { $count ->
    [one] { $count } open incident
   *[other] { $count } open incidents
}
integrity-resolved-count = { $count ->
    [one] { $count } resolved incident
   *[other] { $count } resolved incidents
}
integrity-none-open = No incident is open.
integrity-none-resolved = No incident has been resolved.
integrity-counts-title = Counts since the start
kind-ehr-id-collision = Two nodes claim one EHR id
kind-index-insert-collision = An EHR id was indexed at a second node
kind-learned-conflict = One creating system seen at two nodes
kind-registered-conflict = A registered creating system routes to two nodes
kind-prescription-collision = Two nodes hold one prescription identifier
