- The end-to-end lane holds the production guide's Keycloak recipe (#724): it
  reads the `kcadm.sh` commands, the protocol mapper files and the `[auth]`
  table from the book page, applies them to Keycloak 26.8.0 pinned by digest,
  and gets a federated answer over two FerroEHR nodes with a signed-in
  user's token and a client-credentials token, and a `401` naming `at+jwt`
  with the token-type setting off. The weekly pin-freshness read reports a
  newer Keycloak, and it now follows a registry that pages its tag list.
