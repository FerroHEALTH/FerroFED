# The top bar, the navigation, the user menu and the banner.

lockup-word = Console
operator = Operator
operator-line = Federation operator
nav-label = Console
nav-group-overview = Overview
nav-group-federation = Federation
nav-group-queries = Queries
nav-group-gateway = Gateway
nav-group-console = Console
nav-dashboard = Dashboard
nav-topology = Topology
nav-organisations = Organisations
nav-services = Services
nav-integrity = Integrity
nav-query = Query console
nav-stored = Stored queries
nav-self-description = Self-description
nav-configuration = Configuration
nav-about = About
nav-open-incidents = { $count ->
    [one] { $count } open incident
   *[other] { $count } open incidents
}
nav-incidents-unread = Not known: the gateway did not answer the incidents
nav-incidents-unknown = Not known: the gateway is unreachable
nest-label = { $organisation } and its nodes
nest-worst = Worst node: { $state }
nest-all = { $count ->
    [one] The { $count } node
   *[other] All { $count } nodes
}
gateway-up = Gateway up
gateway-unreachable = Gateway unreachable
gateway-unknown = Gateway unknown
gateway-refused-401 = Gateway refused 401
gateway-refused = Gateway refused { $status }
gateway-detail = FerroFED { $version }
gateway-detail-spec = Federation { $federation } · spec { $spec }
gateway-detail-replica = FerroFED { $version } · answered by replica { $replica }
search-label = Find in the federation
search-placeholder = Organisation, node or query
search-submit = Find
search-disabled = The gateway is unreachable, so there is nothing to find
development-title = Development deployment
development-body = this gateway runs the development profile and must not hold real patient data.
