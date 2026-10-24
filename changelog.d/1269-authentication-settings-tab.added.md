- The running configuration of the admin listener and the operator
  console's new Authentication tab show who may call: the audience, the
  operator scope, each token issuer and each trusted issuer, with every claim
  name, the correlation header and the added countries of a national contact
  point the file declares. The tab is read-only, since only the file sets
  `[auth]` and `[trust]`, and a secret is named by its file alone (#1269).
