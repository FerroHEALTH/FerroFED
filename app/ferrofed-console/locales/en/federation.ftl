# The organisations, nodes, endpoints and services, and their states.

state-reachable = Reachable
state-not-probed = Not probed
state-degraded = Degraded
state-unreachable = Unreachable
state-excluded = Excluded
group-identity = Identity
group-localization = Localization
group-consent = Consent
group-directory = Directory
group-audit = Audit
service-resolver = Identity cross-reference
service-demographics = Demographics
service-identity-registry = Identity feed
service-localizer = Localizer
service-consent = Consent pre-filter
service-directory = Care services directory
service-audit-repository = Audit record repository
service-audit-feed = Audit feed
role-resolver = Resolves a patient to each node's own EHR id.
role-demographics = Finds a patient's master identity before resolution.
role-identity-registry = Tells the gateway when a patient's identity changes.
role-localizer = Finds the nodes that hold a patient's records.
role-consent = Asks whether a patient's records may be shared, before any node is asked.
role-directory = Lists the organisations and their endpoints the registry is read from.
role-audit-repository = Receives the audit record of every transaction.
role-audit-feed = Receives the audit records of the identity transactions.
role-other = A service a binding runs or records through.
organisations-title = Organisations
organisations-subtitle = The organisations in the federation and their nodes.
organisations-find = Find an organisation or a node
organisations-none = No organisation takes part yet.
organisations-add-first = Add the first organisation
organisations-no-match = No organisation matches.
nodes-no-match = No node matches.
new-organisation = New organisation
all-nodes = All nodes
organisation-count = { $count ->
    [one] { $count } organisation
   *[other] { $count } organisations
}
noun-organisations = { $count ->
    [one] organisation
   *[other] organisations
}
    .empty = No organisations
    .opening = Organisations
noun-nodes = { $count ->
    [one] node
   *[other] nodes
}
    .empty = No nodes
    .opening = Nodes
noun-endpoints = { $count ->
    [one] endpoint
   *[other] endpoints
}
    .empty = No endpoints
    .opening = Endpoints
organisation-subtitle = Organisation { $id } · { $count ->
    [one] { $count } node
   *[other] { $count } nodes
}
organisation-facts = Facts
new-node = New node
node-subtitle = Node of { $organisation }
node-facts = Facts
node-health = Health
node-health-line = The last state the gateway observed of each endpoint.
edit-node = Edit
endpoint-count = { $count ->
    [one] { $count } endpoint
   *[other] { $count } endpoints
}
route-own = Its own system id
route-registered = Registered
route-learned = Learned
route-withdrawn = Withdrawn
routes-line = The creating systems whose follow-ups go to this node.
routes-none = No creating system routes to { $node }.
services-title = Services
services-subtitle = The services the gateway asks, with the state the replica last observed.
services-none = The gateway asks no service.
service-health = Health
service-health-line = The last state the replica observed; each replica observes its own.
