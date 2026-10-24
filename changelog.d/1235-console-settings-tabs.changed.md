- The console's configuration page opens on the deployment-wide settings,
  one tab per section as FerroUI's design draws them (Federation
  behaviour, Fan-out and paging, Consent, Stored queries, Access log and
  FHIR face), each setting with its value, its source, its key and its
  Next start mark, read from the gateway's running configuration. Edit on
  a tab saves the changed settings as a revision with a plain form post.
  With configuration revisions off, the settings tabs show read-only.
  `/configuration` now opens Federation behaviour; the members are at
  `/configuration/members` (#1235).
