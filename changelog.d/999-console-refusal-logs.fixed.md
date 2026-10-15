- The operator console logs a query the gateway refuses as a refused query,
  and a configuration action a replica refuses as a refused configuration
  action, naming the action (a member save, an apply, a rollback, a
  stored-query store and the rest) and the replica, with no identifier,
  AQL text or token. Before, a refused query was logged as a refused view
  and a configuration action as an admin action.
