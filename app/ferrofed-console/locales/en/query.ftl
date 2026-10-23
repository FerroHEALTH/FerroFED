# The query console.

query-title = Query console
query-subtitle = Ask the federation as yourself; every endpoint's part is shown beside the rows.
query-tab-aql = AQL
query-tab-stored = Stored query
query-aql-label = AQL
query-aql-help = The query is posted with this page and never reaches an address or the history.
query-name-label = Stored query
query-name-help = The qualified name, such as org.example::vital_signs.
query-version-label = Version
query-version-help = Leave empty for the newest version.
query-parameters = Parameters
query-parameters-help = Each parameter the query names, such as $ehr_id, with its value.
query-parameters-none = No parameters.
query-parameter-name-label = Name
query-parameter-value-label = Value
query-parameter-add = Add a parameter
query-parameter-remove = Remove this parameter
query-offset = Offset
query-fetch = Fetch
query-endpoints = Endpoints
query-endpoints-help = Ask these endpoints only; with none chosen the gateway asks every endpoint in service.
query-organisations = Organisations
query-dedup = Deduplication
query-dedup-default = The gateway's default
query-partial = Take an incomplete answer (best effort)
query-options-every = Federation options: every endpoint in service
query-options-chosen = { $count ->
    [one] Federation options: { $count } chosen
   *[other] Federation options: { $count } chosen
}
query-run = Run
query-before-run = Run a query to see its answer here.
query-answer-title = The answer
query-rows = Rows
query-no-rows = Every asked endpoint answered and none holds a row.
query-no-rows-failed = The answer holds no rows.
query-raw = Raw answer
query-raw-unprintable = The console cannot print this part of the answer.
query-window = { $count ->
    [one] { $count } row
   *[other] { $count } rows
} in { $ms } ms
query-write-aql = Write the AQL query to run.
query-name-stored = Name the stored query to run.
query-header-illegal = A chosen endpoint, organisation or mode cannot be sent as a header.
query-whole-number = { $field } is a whole number from 0.
query-parameter-name = Parameter { $number } needs a name of letters, digits and underscores.
query-parameter-twice = Parameter { $number } names a parameter given before.
