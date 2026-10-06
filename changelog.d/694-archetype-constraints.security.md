- The access log no longer classifies a query by fewer categories than the
  data it delivers (#694). A value a query selects through an archetype
  predicate in its path, such as `c/content[openEHR-EHR-OBSERVATION.….v1]/…`,
  is classified by that archetype too, and an archetype id compared as
  `archetype_details/archetype_id/value` binds its class as
  `archetype_node_id` does. A selected path that names its archetype by a
  pattern, a parameter or any comparison other than `=` marks the record
  `unclassified`. An archetype id written in another case, with another
  version or a namespace is held to the map's exact keys and marked
  `unclassified`, never matched to a near key. `crates/openehr-federation`
  0.0.44 carries the reader.
