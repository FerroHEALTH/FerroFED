- The node profile observes consent before release (N27, CP-19) at
  FerroEHR: the harness sets a restriction of processing (GDPR Art. 18) on
  one of two compositions through FerroEHR's ADMIN API, and the new
  restriction check reads the restricted composition withheld on the read
  and the composition query. An end-to-end case shows the federated query
  through the gateway leaving it out and a follow-up read of it answering
  the node's `403` (#1389).
