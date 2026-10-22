- The quickstart's four member CDRs and the test harness's FerroEHR nodes
  move from FerroEHR 4.3.9 to 5.3.2, the release on the same corrected base,
  with the node database image beside them (#1226). FerroEHR 5.1.0 changed
  its Basic user keys and checks a Basic user's SMART on openEHR scopes on
  every route, so the quickstart's FerroEHR configuration names its user with
  `user`, grants it the three resource-family scopes and FerroEHR's product
  scope, and moves its roles to `[authz.rbac.basic_roles]`; the restricted
  harness node does the same for its two users.
