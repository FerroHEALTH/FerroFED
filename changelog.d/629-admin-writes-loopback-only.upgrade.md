- If you run the admin listener's stored-query distribution, from the
  gateway's host or another, send an access token carrying the
  `operator_scope` of your issuer (#629, #635). Without one it answers
  `401 unauthenticated`, from a loopback peer as from any other, outside
  `profile = "development"`.
