- The audit library and the identity crates move together: `ihe-atna` and
  `ehds-logging` 0.2.1 from FerroAUDIT, `ihe-iti` 0.0.41 from FerroPIX on
  that `ihe-atna`, and `nl-generic-functions` 0.0.26 from FerroPIX on
  `ferrohealth-smart` 0.4.0, so the gateway carries one copy of it. What the
  gateway records, where it sends it and how it spools it are unchanged; the
  log targets of the spool and the forwarder stay `ihe_atna::atna::spool`
  and `ihe_atna::atna::forwarder`, now a stable interface of `ihe-atna`. The
  identity binding's FHIR Feed reaches an `http` base through `ihe-atna`'s
  `FeedRepository::cleartext`, which names the operator's acceptance of clear
  text (#1194).
