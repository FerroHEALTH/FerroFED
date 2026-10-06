- The access log no longer names an endpoint as an origin of data it never
  sent (#731). An endpoint whose request waited out its deadline for a slot
  of `federation.max_in_flight_per_node`, or never left the gateway for
  another reason, is reported `time-out` in the answer and is no origin in
  the record, and a query that left the gateway for no endpoint writes no
  record.
