- Rename `[metrics] scrape_users` to `scrape_clients` and `[pmir]
  feed_users` to `feed_clients`, naming the client ids of your Prometheus
  scraper and your PMIR Registry at an `[[auth.issuer]]` the gateway
  trusts, and remove every `[[auth.basic.users]]` entry: the gateway admits
  no Basic caller, and a configuration that still writes one refuses to
  start, naming the user. Give the scraper an `oauth2` client-credentials
  setup in its scrape job, and the Registry a client-credentials grant at
  that issuer (#1704).
- Move `[trust.issuer.assurance]` to the issuer's `[auth.issuer.assurance]`
  table (`claim`, `minimum`, and a `levels` map from each claim value to
  `low`, `substantial` or `high`), move `[trust.issuer.requester]` to the
  `nl.professional_number`, `nl.role`, `nl.ura` and `nl.organisation_type`
  fields of the caller issuer's `[[auth.federation]]` table, and remove
  `client_tokens_act_for_professional`. Each removed key refuses the start,
  naming its replacement (#1704).
