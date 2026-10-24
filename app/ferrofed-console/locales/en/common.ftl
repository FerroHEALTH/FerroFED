# The words every page shares.

# The digit-group and decimal marks FerroUI's formatter writes a number with.
number-group = ,
number-decimal = .

product = FerroFED Console
cancel = Cancel
try-again = Try again
none = None
technical-detail = Technical detail
in-step = In step
not-in-step = Not in step
no-answer = No answer
sign-in-again = Sign in again
replica-unnamed = a replica that gave no name
replica-of-part = the { $what } by { $replica }
node-count = { $count ->
    [one] { $count } node
   *[other] { $count } nodes
}
ago-seconds = { $count } s ago
ago-minutes = { $count } min ago
ago-hours = { $count } h ago
ago-days = { $count ->
    [one] { $count } day ago
   *[other] { $count } days ago
}
refused-by-gateway = The gateway refused the request.
refused-not-authenticated = The gateway did not accept your sign-in.
fault-unreachable = The console could not reach the gateway.
fault-unreachable-detail = The gateway gave no answer: it may be down, or the network between the console and the gateway may be broken.
fault-unreadable = The gateway answered with something this console cannot read.
fault-unreadable-detail = The gateway answered { $status } with a body of a shape this console does not know. The console and the gateway may be different releases.
fault-signed-out = Your session has ended.
fault-signed-out-detail = Sign in again to see this page.
fault-unavailable = The console could not serve this page.
fault-unavailable-detail = The console could not read its own session store. Whoever runs the console can see why in its log.
author-file = the configuration file
author-loopback = a development loopback
author-gateway = the gateway
author-unknown = someone the console cannot name
column-node = Node
column-nodes = Nodes
column-organisation = Organisation
column-state = State
column-why = Why
column-what-happened = What happened
column-detected = Detected
column-cleared = Cleared
column-ehr-id = EHR id
column-kind = Kind
column-since-start = Since the start
column-id = Id
column-endpoint = Endpoint
column-endpoints = Endpoints
column-system-id = System id
column-cdr = CDR
column-median = p50
column-median-title = The median response time over the last 100 requests the gateway sent, as its self-description reports it
column-p95 = p95
column-p95-title = The 95th percentile response time over the last 100 requests the gateway sent, as its self-description reports it
column-membership = Membership
column-owner = Managed by
column-url = URL
column-worst-node = Worst node
column-creating-system = Creating system
column-ground = Ground
clear-filters = Clear filters
filter-words = Matching “{ $words }”
milliseconds = { $count } ms
no-measure = Not measured yet
tab-overview = Overview
tab-nodes = Nodes
tab-endpoints = Endpoints
tab-routes = Routes
owner-console = Console
owner-file = File
owner-file-title = Set in the configuration file; the console cannot change it
owner-directory = Directory
owner-directory-title = Read from the care services directory; the console cannot change it
owner-gateway = Gateway
owner-gateway-title = The gateway's own; the console cannot change it
owner-store = Store
owner-other = Another owner
column-fault = Fault
column-group = Group
column-key = Key
column-observed-by = Observed by
column-query = Query
column-saved = Saved
column-service = Service
column-version = Version
column-versions = Versions
offered = Offered
not-offered = Not offered
status-active = Answered
status-offline = Offline
status-time-out = Timed out
status-node-error = Node error
status-not-resolved = Not resolved
status-consent-denied = Consent denied
status-excluded = Excluded
status-not-localized = Not localized
status-other = Another status
fanout-label = What each endpoint did
fanout-line = { $answered } of { $asked } asked endpoints answered · status { $status }
fanout-missing = { $endpoints } did not answer, so these rows are not the whole answer.
fanout-rows = { $rows ->
    [one] { $rows } row
   *[other] { $rows } rows
}
fanout-rows-ms = { $rows ->
    [one] { $rows } row
   *[other] { $rows } rows
} · { $ms } ms
fanout-not-asked-measure = not asked
