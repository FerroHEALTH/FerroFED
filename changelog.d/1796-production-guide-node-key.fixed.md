- The production guide's example node grant authenticates with
  `private_key_jwt` and a `private_key_file`, and says how to make that
  key. The `client_secret_basic` grant it showed is refused by
  `ferrofed config check` since a node's grant takes no client secret
  (§13.1, N25).
