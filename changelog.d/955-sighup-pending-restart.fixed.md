- Under `config.ui = "managed"`, a changed configuration file that a
  `SIGHUP` reload records as a revision is marked `pending-restart`, with
  the keys a restart applies, when the reload leaves settings for the next
  start. Before, it was marked `applied`, so `GET /admin/config` claimed a
  configuration ran that only partly did, while a revision saved and
  applied through the API was marked `pending-restart`.
