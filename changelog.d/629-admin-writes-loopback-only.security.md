- The admin listener no longer serves its write actions, such as the
  stored-query distribution, to every peer that reaches it (#629): each now
  needs an access token carrying the issuer's `operator_scope` (#635). The
  Kubernetes example's network policy admits the scraper alone to the admin
  port, as defence in depth.
