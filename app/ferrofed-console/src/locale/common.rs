// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The messages of `locales/en/common.ftl`.

messages! {
    /// `{ $count ->`
    AgoDays = "ago-days",
    /// `… h ago`
    AgoHours = "ago-hours",
    /// `… min ago`
    AgoMinutes = "ago-minutes",
    /// `… s ago`
    AgoSeconds = "ago-seconds",
    /// `the configuration file`
    AuthorFile = "author-file",
    /// `the gateway`
    AuthorGateway = "author-gateway",
    /// `a development loopback`
    AuthorLoopback = "author-loopback",
    /// `someone the console cannot name`
    AuthorUnknown = "author-unknown",
    /// `Cancel`
    Cancel = "cancel",
    /// `Clear filters`
    ClearFilters = "clear-filters",
    /// `CDR`
    ColumnCdr = "column-cdr",
    /// `Cleared`
    ColumnCleared = "column-cleared",
    /// `Creating system`
    ColumnCreatingSystem = "column-creating-system",
    /// `Detected`
    ColumnDetected = "column-detected",
    /// `EHR id`
    ColumnEhrId = "column-ehr-id",
    /// `Endpoint`
    ColumnEndpoint = "column-endpoint",
    /// `Endpoints`
    ColumnEndpoints = "column-endpoints",
    /// `Fault`
    ColumnFault = "column-fault",
    /// `Ground`
    ColumnGround = "column-ground",
    /// `Group`
    ColumnGroup = "column-group",
    /// `Id`
    ColumnId = "column-id",
    /// `Key`
    ColumnKey = "column-key",
    /// `Kind`
    ColumnKind = "column-kind",
    /// `Median`
    ColumnMedian = "column-median",
    /// `The median response over recent requests, as the gateway's …`
    ColumnMedianTitle = "column-median-title",
    /// `Membership`
    ColumnMembership = "column-membership",
    /// `Node`
    ColumnNode = "column-node",
    /// `Nodes`
    ColumnNodes = "column-nodes",
    /// `Observed by`
    ColumnObservedBy = "column-observed-by",
    /// `Organisation`
    ColumnOrganisation = "column-organisation",
    /// `Managed by`
    ColumnOwner = "column-owner",
    /// `Query`
    ColumnQuery = "column-query",
    /// `Saved`
    ColumnSaved = "column-saved",
    /// `Service`
    ColumnService = "column-service",
    /// `Since the start`
    ColumnSinceStart = "column-since-start",
    /// `State`
    ColumnState = "column-state",
    /// `System id`
    ColumnSystemId = "column-system-id",
    /// `URL`
    ColumnUrl = "column-url",
    /// `Version`
    ColumnVersion = "column-version",
    /// `Versions`
    ColumnVersions = "column-versions",
    /// `What happened`
    ColumnWhatHappened = "column-what-happened",
    /// `Why`
    ColumnWhy = "column-why",
    /// `Worst node`
    ColumnWorstNode = "column-worst-node",
    /// `What each endpoint did`
    FanoutLabel = "fanout-label",
    /// `… of … asked endpoints answered · status …`
    FanoutLine = "fanout-line",
    /// `… did not answer, so these rows are not the whole answer.`
    FanoutMissing = "fanout-missing",
    /// `not asked`
    FanoutNotAskedMeasure = "fanout-not-asked-measure",
    /// `{ $rows ->`
    FanoutRows = "fanout-rows",
    /// `{ $rows ->`
    FanoutRowsMs = "fanout-rows-ms",
    /// `Your session has ended.`
    FaultSignedOut = "fault-signed-out",
    /// `Sign in again to see this page.`
    FaultSignedOutDetail = "fault-signed-out-detail",
    /// `The console could not serve this page.`
    FaultUnavailable = "fault-unavailable",
    /// `The console could not read its own session store. Whoever runs the …`
    FaultUnavailableDetail = "fault-unavailable-detail",
    /// `The console could not reach the gateway.`
    FaultUnreachable = "fault-unreachable",
    /// `The gateway gave no answer: it may be down, or the network between …`
    FaultUnreachableDetail = "fault-unreachable-detail",
    /// `The gateway answered with something this console cannot read.`
    FaultUnreadable = "fault-unreadable",
    /// `The gateway answered … with a body of a shape this console does …`
    FaultUnreadableDetail = "fault-unreadable-detail",
    /// `Matching “…”`
    FilterWords = "filter-words",
    /// `In step`
    InStep = "in-step",
    /// `… ms`
    Milliseconds = "milliseconds",
    /// `No answer`
    NoAnswer = "no-answer",
    /// `Not measured yet`
    NoMeasure = "no-measure",
    /// `{ $count ->`
    NodeCount = "node-count",
    /// `None`
    None = "none",
    /// `Not in step`
    NotInStep = "not-in-step",
    /// `Not offered`
    NotOffered = "not-offered",
    /// `Offered`
    Offered = "offered",
    /// `Console`
    OwnerConsole = "owner-console",
    /// `Directory`
    OwnerDirectory = "owner-directory",
    /// `Read from the care services directory; the console cannot change it`
    OwnerDirectoryTitle = "owner-directory-title",
    /// `File`
    OwnerFile = "owner-file",
    /// `Set in the configuration file; the console cannot change it`
    OwnerFileTitle = "owner-file-title",
    /// `Gateway`
    OwnerGateway = "owner-gateway",
    /// `The gateway's own; the console cannot change it`
    OwnerGatewayTitle = "owner-gateway-title",
    /// `Another owner`
    OwnerOther = "owner-other",
    /// `Store`
    OwnerStore = "owner-store",
    /// `FerroFED Console`
    Product = "product",
    /// `The gateway refused the request.`
    RefusedByGateway = "refused-by-gateway",
    /// `The gateway did not accept your sign-in.`
    RefusedNotAuthenticated = "refused-not-authenticated",
    /// `the … by …`
    ReplicaOfPart = "replica-of-part",
    /// `a replica that gave no name`
    ReplicaUnnamed = "replica-unnamed",
    /// `Sign in again`
    SignInAgain = "sign-in-again",
    /// `Answered`
    StatusActive = "status-active",
    /// `Consent denied`
    StatusConsentDenied = "status-consent-denied",
    /// `Excluded`
    StatusExcluded = "status-excluded",
    /// `Node error`
    StatusNodeError = "status-node-error",
    /// `Not localized`
    StatusNotLocalized = "status-not-localized",
    /// `Not resolved`
    StatusNotResolved = "status-not-resolved",
    /// `Offline`
    StatusOffline = "status-offline",
    /// `Another status`
    StatusOther = "status-other",
    /// `Timed out`
    StatusTimeOut = "status-time-out",
    /// `Endpoints`
    TabEndpoints = "tab-endpoints",
    /// `Nodes`
    TabNodes = "tab-nodes",
    /// `Overview`
    TabOverview = "tab-overview",
    /// `Routes`
    TabRoutes = "tab-routes",
    /// `Technical detail`
    TechnicalDetail = "technical-detail",
    /// `Try again`
    TryAgain = "try-again",
}
