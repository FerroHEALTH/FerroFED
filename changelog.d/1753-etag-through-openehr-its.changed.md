- The id a CDR names in the `ETag` of a new EHR or a new version is read
  with `openehr-its`'s entity tag, weak or strong, into the typed
  `HIER_OBJECT_ID` or `OBJECT_VERSION_ID`. A tag that names no such id is
  passed over for the `Location`, where any non-empty tag was taken before.
