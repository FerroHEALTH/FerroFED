// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The messages of the gateway views: members, routing, integrity, stored
//! queries and the self-description.

/// Appends these messages to the declaration `$acc`, then hands it to the
/// next macro of the chain `[$next, $rest...]`.
macro_rules! views_messages {
    ([$next:ident $(, $rest:ident)*] $($acc:tt)*) => {
        $next! {
            [$($rest),*]
            $($acc)*
            /// `Declared endpoints`
            DeclaredEndpoints = "declared-endpoints",
            /// `Membership`
            Membership = "membership",
            /// `Fixes until`
            FixesUntil = "fixes-until",
            /// `no end date`
            NoEndDate = "no-end-date",
            /// `declared in a form the console cannot read`
            DeclaredFormConsoleCannotRead = "declared-form-console-cannot-read",
            /// `not declared`
            NotDeclared = "not-declared",
            /// `Decomposable aggregates`
            DecomposableAggregates = "decomposable-aggregates",
            /// `Offset strategy`
            OffsetStrategy = "offset-strategy",
            /// `Timeout policy`
            TimeoutPolicy = "timeout-policy",
            /// `Overall`
            Overall = "overall",
            /// `Per node`
            PerNode = "per-node",
            /// `On a localization failure`
            OnALocalizationFailure = "on-a-localization-failure",
            /// `Template upload fan-out`
            TemplateUploadFanOut = "template-upload-fan-out",
            /// `Stored-query fan-out`
            StoredQueryFanOut = "stored-query-fan-out",
            /// `Stored-query registry`
            StoredQueryRegistry = "stored-query-registry",
            /// `none declared`
            NoneDeclared = "none-declared",
            /// `Deduplication header`
            DeduplicationHeader = "deduplication-header",
            /// `Deduplication modes`
            DeduplicationModes = "deduplication-modes",
            /// `AQL fan-out`
            AqlFanOut = "aql-fan-out",
            /// `Demographic`
            Demographic = "demographic",
            /// `Definition`
            Definition = "definition",
            /// `EHR`
            Ehr = "ehr",
            /// `JWK Set`
            JwkSet = "jwk-set",
            /// `Member endpoints`
            MemberEndpoints = "member-endpoints",
            /// `Specification version`
            SpecificationVersion = "specification-version",
            /// `not offered`
            NotOffered = "not-offered",
            /// `The OPTIONS document`
            TheOptionsDocument = "the-options-document",
            /// `ITS-REST areas`
            ItsRestAreas = "its-rest-areas",
            /// `Budgets and paging`
            BudgetsAndPaging = "budgets-and-paging",
            /// `Capabilities`
            Capabilities = "capabilities",
            /// `Loading the self-description.`
            LoadingTheSelfDescription = "loading-the-self-description",
            /// `What the gateway declares of itself in OPTIONS.`
            WhatGatewayDeclaresItselfOptions = "what-gateway-declares-itself-options",
            /// `Collisions cleared recently, newest first`
            CollisionsClearedRecentlyNewestFirst = "collisions-cleared-recently-newest-first",
            /// `Members that claimed it`
            MembersThatClaimedIt = "members-that-claimed-it",
            /// `not a bare UUID`
            NotABareUuid = "not-a-bare-uuid",
            /// `Cleared at`
            ClearedAt = "cleared-at",
            /// `No collision cleared since the gateway started.`
            NoCollisionClearedSinceGatewayStarted = "no-collision-cleared-since-gateway-started",
            /// `Collisions cleared recently`
            CollisionsClearedRecently = "collisions-cleared-recently",
            /// `Recent incidents, newest first`
            RecentIncidentsNewestFirst = "recent-incidents-newest-first",
            /// `Description`
            Description = "description",
            /// `None of the incidents this replica keeps is open.`
            NoneIncidentsReplicaKeepsOpen = "none-incidents-replica-keeps-open",
            /// `No open incidents`
            NoOpenIncidents = "no-open-incidents",
            /// `No collision held here was cleared since it was found.`
            NoCollisionHeldHereWasCleared = "no-collision-held-here-was-cleared",
            /// `No resolved incidents`
            NoResolvedIncidents = "no-resolved-incidents",
            /// `Resolved`
            Resolved = "resolved",
            /// `Open`
            Open = "open",
            /// `Incidents by state`
            IncidentsByState = "incidents-by-state",
            /// `Incidents since the gateway started`
            IncidentsSinceGatewayStarted = "incidents-since-gateway-started",
            /// `Count`
            Count = "count",
            /// `Loading the incidents.`
            LoadingTheIncidents = "loading-the-incidents",
            /// `The incidents this replica keeps, and the collisions cleared.`
            IncidentsReplicaKeepsCollisionsCleared = "incidents-replica-keeps-collisions-cleared",
            /// `The gateway did not name the replica that answered.`
            GatewayDidNotNameReplicaAnswered = "gateway-did-not-name-replica-answered",
            /// `routing table`
            RoutingTable = "routing-table",
            /// `No such member`
            NoSuchMember = "no-such-member",
            /// `Endpoints of this member`
            EndpointsOfThisMember = "endpoints-of-this-member",
            /// `Loading the member.`
            LoadingTheMember = "loading-the-member",
            /// `Other dependencies`
            OtherDependencies = "other-dependencies",
            /// `Dependency`
            Dependency = "dependency",
            /// `Loading the members.`
            LoadingTheMembers = "loading-the-members",
            /// `Each member endpoint and its health, as one replica observes it.`
            EachMemberEndpointHealthOneReplica = "each-member-endpoint-health-one-replica",
            /// `Sign in to see this view.`
            SignSeeView = "sign-see-view",
            /// `Pages`
            Pages = "pages",
            /// `Filter`
            Filter = "filter",
            /// `The creating_system_id routing table`
            CreatingSystemIdRoutingTable = "creating-system-id-routing-table",
            /// `No row on this page matches the filter.`
            NoRowPageMatchesFilter = "no-row-page-matches-filter",
            /// `Source`
            Source = "source",
            /// `Loading the routing table.`
            LoadingTheRoutingTable = "loading-the-routing-table",
            /// `Where each creating_system_id routes, and on what ground.`
            WhereEachCreatingSystemIdRoutes = "where-each-creating-system-id-routes",
            /// `Loading the stored query.`
            LoadingTheStoredQuery = "loading-the-stored-query",
            /// `Loading the stored queries.`
            LoadingTheStoredQueries = "loading-the-stored-queries",
            /// `The queries the gateway holds, by namespace.`
            QueriesGatewayHoldsNamespace = "queries-gateway-holds-namespace",
            /// `No such stored query`
            NoSuchStoredQuery = "no-such-stored-query",
            /// `Back to the stored queries`
            BackStoredQueries = "back-stored-queries",
            /// `stored queries`
            StoredQueries2 = "stored-queries-2",
            /// `shown`
            Shown = "shown",
            /// `Details`
            Details = "details",
            /// `Stored`
            Stored = "stored",
            /// `Namespace`
            Namespace = "namespace",
            /// `Open in query console`
            OpenInQueryConsole = "open-in-query-console",
            /// `No namespace`
            NoNamespace = "no-namespace",
            /// `1 version`
            N1Version = "n-1-version",
            /// `Namespaces`
            Namespaces = "namespaces",
            /// `No stored queries`
            NoStoredQueries = "no-stored-queries",
            /// `The gateway holds no stored query.`
            GatewayHoldsNoStoredQuery = "gateway-holds-no-stored-query",
            /// offered, by `$value0`
            OfferedBy = "offered-by",
            /// `$per_node_ms ms`
            Ms5 = "ms-5",
            /// `$overall_ms ms`
            Ms6 = "ms-6",
            /// Answered by replica `$replica`.
            AnsweredByReplica = "answered-by-replica",
            /// the `$what` by a replica that has no name
            ReplicaHasNoName = "replica-has-no-name",
            /// the `$what` by replica `$replica`
            TheByReplica = "the-by-replica",
            /// the `$what` by a replica that gave no name
            ReplicaGaveNoName = "replica-gave-no-name",
            /// Each part names its replica: `$parts`.
            EachPartNamesReplica = "each-part-names-replica",
            /// `$latency ms`
            Ms7 = "ms-7",
            /// Member `$value0`
            Member = "member",
            /// A member of `$organisation`, with its endpoints and their health.
            MemberEndpointsTheirHealth = "member-endpoints-their-health",
            /// No row on this page, of `$total` in all.
            NoRowPageAll = "no-row-page-all",
            /// Rows `$offset` to `$end` of `$total`.
            RowsToOf2 = "rows-to-of-2",
            /// The gateway did not accept your sign-in (`$code`).
            GatewayDidNotAcceptYourSign2 = "gateway-did-not-accept-your-sign-2",
            /// The gateway refused this view: `$status` (`$code`).`$hint`
            GatewayRefusedView = "gateway-refused-view",
            /// `$kept of $on_page rows on this page.`
            RowsPage = "rows-page",
            /// `$n versions`
            Versions2 = "versions-2",
            /// The gateway holds no version `$asked` of this query. The newest is shown.
            GatewayHoldsNoVersionQueryNewest = "gateway-holds-no-version-query-newest",
            /// Version `$version`, stored `$saved`.
            VersionStored = "version-stored",
            /// The AQL of version `$version`
            TheAqlOfVersion = "the-aql-of-version",
            /// The gateway holds no stored query named `$name`.
            GatewayHoldsNoStoredQueryNamed = "gateway-holds-no-stored-query-named",
            /// A capability the gateway offers.
            Offered = "offered",
            /// `404: the gateway declares no member at this address.`
            NoMemberAtAddress = "no-member-at-address",
            /// The badge on a stored query the gateway serves read-only.
            ReadOnlyBadge = "read-only-badge",
        }
    };
}
