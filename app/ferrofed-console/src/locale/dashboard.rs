// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The messages of `locales/en/dashboard.ftl`.

messages! {
    /// `All nodes`
    AttentionAll = "attention-all",
    /// `Every node that takes part answers.`
    AttentionNone = "attention-none",
    /// `Nodes that need attention`
    AttentionTitle = "attention-title",
    /// `The console is configured with no admin listener.`
    ConfigNoAdmin = "config-no-admin",
    /// `Replicas`
    ConfigReplicas = "config-replicas",
    /// `Revisions`
    ConfigRevisions = "config-revisions",
    /// `Running`
    ConfigRunning = "config-running",
    /// `Revision … · applied … by …`
    ConfigRunningValue = "config-running-value",
    /// `Stored queries`
    ConfigStored = "config-stored",
    /// `{ $count ->`
    ConfigStoredValue = "config-stored-value",
    /// `Configuration`
    ConfigurationTitle = "configuration-title",
    /// `No organisation takes part in the federation yet.`
    DashboardEmpty = "dashboard-empty",
    /// `Organisations`
    DashboardEmptyLink = "dashboard-empty-link",
    /// `… rows`
    DashboardRows = "dashboard-rows",
    /// `Give the endpoint the credentials the gateway reaches it with.`
    DashboardSetupCredentials = "dashboard-setup-credentials",
    /// `Add an organisation with its first node and that node's first …`
    DashboardSetupOrganisation = "dashboard-setup-organisation",
    /// `Run a first query in the query console.`
    DashboardSetupQuery = "dashboard-setup-query",
    /// `The federation as the gateway sees it.`
    DashboardSubtitle = "dashboard-subtitle",
    /// `The federation as replica … sees it.`
    DashboardSubtitleReplica = "dashboard-subtitle-replica",
    /// `Dashboard`
    DashboardTitle = "dashboard-title",
    /// `No incident is open.`
    IncidentsNoneOpen = "incidents-none-open",
    /// `Open incidents`
    IncidentsOpenTitle = "incidents-open-title",
    /// `Managed`
    ModeManaged = "mode-managed",
    /// `{ $count ->`
    StatHeld = "stat-held",
    /// `{ $count ->`
    StatInStep = "stat-in-step",
    /// `No revision`
    StatNoRevision = "stat-no-revision",
    /// `{ $nodes ->`
    StatNodesEndpoints = "stat-nodes-endpoints",
    /// `Nodes reachable`
    StatNodesReachable = "stat-nodes-reachable",
    /// `… of …`
    StatOf = "stat-of",
    /// `Open incidents`
    StatOpenIncidents = "stat-open-incidents",
    /// `{ $count ->`
    StatOrganisations = "stat-organisations",
    /// `Revision …`
    StatRevision = "stat-revision",
    /// `Running configuration`
    StatRunningConfiguration = "stat-running-configuration",
    /// `… · …`
    StatWorstNode = "stat-worst-node",
    /// `… organisations · … nodes: … reachable, … degraded, … unreachable, …`
    TopologyTally = "topology-tally",
    /// `Topology`
    TopologyTitle = "topology-title",
    /// `the last request failed`
    WhyDegraded = "why-degraded",
    /// `its membership is not active`
    WhyExcluded = "why-excluded",
    /// `the last request got no answer`
    WhyUnreachable = "why-unreachable",
}
