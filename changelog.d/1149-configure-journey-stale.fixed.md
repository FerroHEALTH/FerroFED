- The configuration journeys read the focused element and the credential's
  value field afresh while they wait, so a re-render after a save no longer
  fails them with a stale element (#1149).
