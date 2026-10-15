- Under `config.ui = "managed"`, a revision applied or rolled back on one
  replica takes effect on every replica that shares the revision store.
  Each replica reads the running mark every `config.poll_ms` (5000 ms by
  default, 100 to 600000) and applies a revision it does not run through
  the reload path. A replica that refuses the revision keeps its running
  configuration, logs the refusal and counts it in
  `ferrofed_config_revision_follows_total`. `GET /admin/config` names the
  revision the replica runs as `running`, whether it is the current one as
  `in_step`, and a revision it refused as `refused`. The gauge
  `ferrofed_config_revision_in_step` and the alert
  `FerroFEDConfigurationDrift` report a replica out of step.
