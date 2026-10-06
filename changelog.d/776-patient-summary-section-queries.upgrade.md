- Move any stored-query definition you hold in the namespace
  `eu.ferrofed.eehrxf`, or a namespace under it, to a namespace of your own
  before you upgrade (#776). The gateway now reserves that namespace for its
  own read-only queries: a `redb` or PostgreSQL store, or a definition file,
  holding one there refuses the start, and `config check` refuses such a
  file.
