# The dashboard.

dashboard-title = Dashboard
dashboard-subtitle = The federation as the gateway sees it.
dashboard-subtitle-replica = The federation as replica { $replica } sees it.
dashboard-rows = { $count } rows
dashboard-empty = No organisation takes part in the federation yet.
dashboard-empty-link = Organisations
dashboard-setup-organisation = Add an organisation with its first node and that node's first endpoint.
dashboard-setup-credentials = Give the endpoint the credentials the gateway reaches it with.
dashboard-setup-query = Run a first query in the query console.
stat-organisations = { $count ->
    [one] Organisation
   *[other] Organisations
}
stat-nodes-endpoints = { $nodes ->
    [one] { $nodes } node
   *[other] { $nodes } nodes
}, { $endpoints ->
    [one] { $endpoints } endpoint
   *[other] { $endpoints } endpoints
}
stat-of = { $count } of { $total }
stat-nodes-reachable = Nodes reachable
stat-worst-node = { $node } · { $state }
stat-open-incidents = Open incidents
stat-held = { $count ->
    [one] { $count } EHR id held
   *[other] { $count } EHR ids held
}
stat-revision = Revision { $revision }
stat-no-revision = No revision
stat-running-configuration = Running configuration
stat-in-step = { $count ->
    [one] In step on { $count } replica
   *[other] In step on { $count } replicas
}
attention-title = Nodes that need attention
attention-all = All nodes
attention-none = Every node that takes part answers.
why-excluded = its membership is not active
why-unreachable = the last request got no answer
why-degraded = the last request failed
topology-title = Topology
topology-tally = { $organisations } organisations · { $nodes } nodes: { $reachable } reachable, { $degraded } degraded, { $unreachable } unreachable, { $excluded } excluded · { $services } services
incidents-open-title = Open incidents
incidents-none-open = No incident is open.
configuration-title = Configuration
config-revisions = Revisions
config-no-admin = The console is configured with no admin listener.
config-running = Running
config-running-value = Revision { $revision } · applied { $at } by { $by }
config-replicas = Replicas
config-stored = Stored queries
config-stored-value = { $count ->
    [one] { $count } stored version
   *[other] { $count } stored versions
}
mode-managed = Managed
