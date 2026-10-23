# The stored queries.

stored-title = Stored queries
stored-subtitle = The queries the gateway holds, by namespace, each with its versions.
stored-none = The gateway holds no stored query.
stored-new-version = New stored query version
stored-gateway-owned = The gateway's own namespace; the console cannot change it
stored-subtitle-one = Version { $version } shown.
stored-run = Run in the query console
stored-tab-aql = AQL
stored-tab-versions = Versions
stored-version-count = { $count ->
    [one] { $count } version
   *[other] { $count } versions
}
stored-version-not-held = Version { $version } is not held; the newest is shown.
stored-aql-of = The AQL of version { $version }
stored-read-only = Read-only
stored-read-only-title = Read from the definition files; the console cannot change it
repair-action = Send again
repair-title = Send a version again to the endpoints
repair-endpoints = Endpoints
repair-endpoints-help = * for every active endpoint, or endpoint ids separated by commas.
repair-replica = Sent by replica { $replica }; the stored-query store is the replicas' shared one.
repair-submit = Send again
repair-what = drift repair
repair-confirm = Confirm the drift repair in its confirmation step to run it.
repair-no-admin = The console is configured with no admin listener, so it offers no repair.
repair-name-version = Name the version to send again.
repair-endpoints-illegal = The endpoints cannot be sent as a header.
repair-done = Version { $version } was sent again by { $replica }, and every endpoint took it.
repair-partly = Version { $version } was sent again by { $replica }, and some endpoints did not take it.
