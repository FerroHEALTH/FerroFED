- The compose quickstart and the end-to-end harness run FerroEHR 5.6.3,
  node and database image alike, pinned by digest, in place of 5.5.0. The
  harness sets a restriction of processing at a node through
  `/ferroehr/rest/extensions/v1/admin/restriction`, where FerroEHR 5.6.0
  moved its own extension routes (#1617).
