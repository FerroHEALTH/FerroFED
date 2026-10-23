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
topology-median = median { $count } ms
topology-open-page = Open its page
