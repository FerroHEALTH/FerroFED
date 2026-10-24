# The topology.

topology-subtitle = The gateway, the services it asks and the organisations with their nodes.
topology-no-stale = No map is shown while the gateway does not answer, so a stale one never looks current.
topology-gateway = Gateway
topology-map-label = The federation as a map
topology-list-label = The federation as a list
topology-services = Services
topology-services-line = that the gateway asks
topology-organisations = Organisations
topology-organisations-line = and their nodes
topology-org-line = { $count ->
    [one] { $count } node
   *[other] { $count } nodes
} · { $state }
topology-median = p50 { $count } ms
topology-latency = p50 { $p50 } ms · p95 { $p95 } ms
topology-open-page = Open its page
topology-rule-up = the last probe got a status below 500 within half the per-node budget
topology-rule-degraded = it answered with a 5xx, or slower than half the budget
topology-rule-unreachable = no answer within the per-node budget
topology-rule-not-checked = no probe has run yet: a state, never a value
topology-rule-off = excluded, or its organisation excluded: asked nothing
topology-rule-note = An organisation, a service group and the branch into each show the worst of what they hold, and only a degraded, unreachable or slow branch carries its latency. No specification governs this rule: it is FerroFED's own design.
