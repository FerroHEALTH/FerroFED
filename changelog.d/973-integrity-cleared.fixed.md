- The operator console's integrity view lists the collisions an operator
  cleared recently, newest first, as the gateway reports them beside its
  incidents. Before, the view left them out. A replica's refusal of an
  admin action, a body the console cannot read, and an admin listener it
  cannot reach are each logged as that action, with the replica, the
  status and the code, and never the `ehr_id` or the stored query the
  operator named. Before, each was logged as a view the gateway refused.
