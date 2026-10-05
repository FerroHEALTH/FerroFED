- The PIXm resolver reads an ITI-83 answer that repeats one identical
  `(system, value)` `targetIdentifier` in a member's domain as that one
  identifier, so a PIX Manager that returns the same identifier from a master
  and a local record no longer fails the query with `424` (PIXm 3.1.0
  §2:3.83.4.2.2.1, §5.2). Two different values in one domain stay ambiguous
  (#679).
