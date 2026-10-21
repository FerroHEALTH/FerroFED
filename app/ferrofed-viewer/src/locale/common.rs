// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The messages more than one area draws, and those of the shell, the
//! dashboard, the home, sign-in and about pages.

/// Appends these messages to the declaration `$acc`, then hands it to the
/// next macro of the chain `[$next, $rest...]`.
macro_rules! common_messages {
    ([$next:ident $(, $rest:ident)*] $($acc:tt)*) => {
        $next! {
            [$($rest),*]
            $($acc)*
            /// The title of the page for a session store the console cannot read.
            UnreadableTitle = "unreadable-title",
            /// The heading of that page.
            UnreadableHeading = "unreadable-heading",
            /// The fault that page states.
            UnreadableFault = "unreadable-fault",
            /// That page's way to ask again.
            UnreadableRetry = "unreadable-retry",
            /// The title and heading of the page for a path the console does not serve.
            NotFoundTitle = "not-found-title",
            /// That page's way back.
            NotFoundBack = "not-found-back",
            /// `The product, the gateway it reads, and who makes it.`
            ProductGatewayReadsWhoMakes = "product-gateway-reads-who-makes",
            /// `About`
            About = "about",
            /// `Website`
            Website = "website",
            /// `Contact`
            Contact = "contact",
            /// `Postal address`
            PostalAddress = "postal-address",
            /// `Name`
            Name = "name",
            /// `Manufacturer`
            Manufacturer = "manufacturer",
            /// `Gateway`
            Gateway = "gateway",
            /// `Purpose`
            Purpose = "purpose",
            /// `Version`
            Version = "version",
            /// `Product`
            Product = "product",
            /// `Sign in to see the gateway this console reads.`
            SignSeeGatewayConsoleReads = "sign-see-gateway-console-reads",
            /// `Sign in again`
            SignInAgain = "sign-in-again",
            /// `no code`
            NoCode = "no-code",
            ///  Your access token may carry no operator scope.
            YourAccessTokenMayCarryNo = "your-access-token-may-carry-no",
            /// `Stored query name`
            StoredQueryName = "stored-query-name",
            /// `The form is ready once the page has loaded.`
            FormReadyOncePageHasLoaded = "form-ready-once-page-has-loaded",
            /// `Cancel`
            Cancel = "cancel",
            /// `Learn more`
            LearnMore = "learn-more",
            /// `Operator`
            Operator = "operator",
            /// `Configuration`
            Configuration = "configuration",
            /// `Value`
            Value = "value",
            /// `Endpoint`
            Endpoint = "endpoint",
            /// `State`
            State = "state",
            /// `Members`
            Members = "members",
            /// `Node`
            Node = "node",
            /// `Organisation`
            Organisation = "organisation",
            /// `Endpoints`
            Endpoints = "endpoints",
            /// `System id`
            SystemId = "system-id",
            /// `Console`
            Console = "console",
            /// `Stored queries`
            StoredQueries = "stored-queries",
            /// `AQL`
            Aql = "aql",
            /// `Versions`
            Versions = "versions",
            /// `All incidents`
            AllIncidents = "all-incidents",
            /// `Recent incidents`
            RecentIncidents = "recent-incidents",
            /// `At`
            At = "at",
            /// `None since the gateway started.`
            NoneSinceGatewayStarted = "none-since-gateway-started",
            /// `No incidents`
            NoIncidents = "no-incidents",
            /// `All members`
            AllMembers = "all-members",
            /// `Member health`
            MemberHealth = "member-health",
            /// `The gateway declares no member endpoint.`
            GatewayDeclaresNoMemberEndpoint = "gateway-declares-no-member-endpoint",
            /// `No members`
            NoMembers = "no-members",
            /// `Median latency`
            MedianLatency = "median-latency",
            /// `Health`
            Health = "health",
            /// `Kind`
            Kind = "kind",
            /// `Running revision`
            RunningRevision = "running-revision",
            /// `Open incidents`
            OpenIncidents = "open-incidents",
            /// `Nodes up`
            NodesUp = "nodes-up",
            /// `Members active`
            MembersActive = "members-active",
            /// `not shown`
            NotShown = "not-shown",
            /// `not yet observed`
            NotYetObserved = "not-yet-observed",
            /// `all up`
            AllUp = "all-up",
            /// `no nodes`
            NoNodes = "no-nodes",
            /// `not reported`
            NotReported = "not-reported",
            /// `Loading the dashboard.`
            LoadingTheDashboard = "loading-the-dashboard",
            /// `Dashboard`
            Dashboard = "dashboard",
            /// `The state of the federation, as this console reads it.`
            StateFederationConsoleReads = "state-federation-console-reads",
            /// `Sign in to the console`
            SignConsole = "sign-console",
            /// `Sign in`
            SignIn = "sign-in",
            /// `none`
            None = "none",
            /// `Best effort`
            BestEffort = "best-effort",
            /// `Query`
            Query = "query",
            /// `Query console`
            QueryConsole = "query-console",
            /// `Views`
            Views = "views",
            /// `Theme not kept`
            ThemeNotKept = "theme-not-kept",
            /// `the console refused it`
            TheConsoleRefusedIt = "the-console-refused-it",
            /// `Development deployment`
            DevelopmentDeployment = "development-deployment",
            ///  Patients resolve through the static development cross-reference.
            PatientsResolveThroughStaticDevelopmentCross = "patients-resolve-through-static-development-cross",
            /// `this gateway runs the development profile and must not hold real patient data.`
            GatewayRunsDevelopmentProfileMustNot = "gateway-runs-development-profile-must-not",
            /// `Gateway UNKNOWN`
            GatewayUnknown = "gateway-unknown",
            /// `Gateway DOWN`
            GatewayDown = "gateway-down",
            /// `Gateway refused 401`
            GatewayRefused401 = "gateway-refused-401",
            /// `Gateway UP`
            GatewayUp = "gateway-up",
            /// `Signed in to this console`
            SignedConsole = "signed-console",
            /// `Queries`
            Queries = "queries",
            /// `Federation`
            Federation = "federation",
            /// `Overview`
            Overview = "overview",
            /// `Self-description`
            SelfDescription = "self-description",
            /// `Integrity`
            Integrity = "integrity",
            /// `Routing`
            Routing = "routing",
            /// The About page's purpose line.
            AboutPurpose = "about-purpose",
            /// The gateway refused this part: `$status` (`$code`).
            GatewayRefusedPart = "gateway-refused-part",
            /// The gateway did not accept your sign-in (`$code`).
            GatewayDidNotAcceptYourSign = "gateway-did-not-accept-your-sign",
            /// Not shown: `$fault`.
            NotShown2 = "not-shown-2",
            /// `$down down`
            Down = "down",
            /// `$active of $total`
            Of = "of",
            /// `$up of $total`
            Of2 = "of-2",
            /// revision `$pending` pending
            RevisionPending = "revision-pending",
            /// `$latency ms`
            Ms = "ms",
            /// · v`$version` · `$replica`
            V = "v",
            /// · v`$version`
            V2 = "v-2",
            /// · spec `$spec_version` · `$federation`
            Spec = "spec",
            /// Gateway refused `$status`
            GatewayRefused = "gateway-refused",
            /// An incident that is resolved.
            StateResolved = "state-resolved",
            /// An incident that is open.
            StateOpen = "state-open",
            /// The sentence of that page.
            NotFoundBody = "not-found-body",
        }
    };
}
