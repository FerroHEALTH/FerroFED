- The console's query console is an editor card and a results card (#1078).
  The editor has AQL and Stored query tabs, parameters as name and value
  rows, offset and fetch on one row, and federation options in which the
  declared endpoints and organisations are checkbox chips and best effort is
  a switch. The results card shows a summary strip, one chip per node with
  its status, rows and latency, the rows in pages of 25 under a header that
  stays, the raw result set on its own tab, and a refused query as the
  card's error state. The focus moves to the results after a run.
