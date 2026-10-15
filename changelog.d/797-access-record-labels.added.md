- Each access record carries a security label per category of the data it
  reached and per origin of the data, in `meta.security` and on its
  `ehds-categories` and `origin` entities, so your Audit Record Repository
  can limit who reads which records by category and origin (Regulation
  (EU) 2025/327 Annex II 3.4). A category is labelled with its own code
  and an origin with its endpoint id in
  `https://ferrofed.eu/fhir/CodeSystem/access-log-origin` unless
  `[access_log.labels]` declares other labels, and an unclassified access
  carries every category's label. A malformed label, or one for a
  category or endpoint the gateway does not know, is refused when the
  configuration loads (#797).
