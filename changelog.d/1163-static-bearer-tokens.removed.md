- Static bearer tokens, inbound and outbound: `bearer_token` and
  `bearer_token_file` in every credentials section, `[metrics]
  scrape_token` and `scrape_token_file`, and `[pmir] feed_token` and
  `feed_token_file` (#1163). Machine-to-machine authentication is HTTP Basic
  or the OAuth 2.0 client credentials grant.
