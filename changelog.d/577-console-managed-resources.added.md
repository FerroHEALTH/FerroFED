- Under `config.ui = "managed"`, the operator console manages members and
  stored queries beside the file (#577). Every member and stored-query
  version has one owner, which `GET /admin/config/resources` names: a member
  the registry document or a care services directory provides is read-only,
  and a console member, its organisation, node and endpoints together, is
  held in the revisions as `[[registry.members]]` and joined to the
  document's. `PUT /admin/config/members/{node_id}` and
  `POST /admin/config/members/{node_id}/remove` save a revision you preview
  with the diff and then apply or roll back; a change to a member the file
  provides is refused `409 resource-file-owned`, and the removal of no
  console member `404 resource-unknown`. A console member survives a
  `SIGHUP` reload, and when the file later provides the same member the
  file's runs and the console's copy is reported as shadowed.
  `PUT /admin/config/stored-queries/{name}/{version}` stores a stored-query
  version, which is never edited or removed (§12.7, N44). The console's new
  Configuration page shows the revisions, the owners and the history, with a
  confirmed form for each change. The configuration file never sets
  `registry.members`.
