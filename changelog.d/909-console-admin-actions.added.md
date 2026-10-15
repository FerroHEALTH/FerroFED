- The operator console runs the operator's two admin actions on the
  gateway's admin listener (#909): the clear of a held `ehr_id` collision,
  below the integrity incidents, at one replica or at every replica in turn,
  and the stored-query drift repair, below the stored queries, through the
  first replica. The console's new `[[admin.replicas]]` table names each
  replica's `admin_url`, which its server calls with the signed-in
  operator's own access token; without it no action is offered. Each action
  asks for a confirmation and names the replica it acted on, and a refusal
  shows its status and stable error code. The admin listener must then bind
  an internal address the console's server can reach, never a public one.
