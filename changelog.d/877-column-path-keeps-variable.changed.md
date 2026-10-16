- `columns[].path` of a federated result set keeps the AQL variable as the
  client wrote it, `c/uid/value` and `p/id`, as the Federation Tier §9.4
  example renders it, where it was `/uid/value` and `/id` before. Two
  variables of one class, such as `c1/uid/value` and `c2/uid/value`, now
  render apart. A client that matched the stripped form reads the variable
  first.
