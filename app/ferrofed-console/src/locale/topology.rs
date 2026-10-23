// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The messages of `locales/en/topology.ftl`.

messages! {
    /// `Gateway`
    TopologyGateway = "topology-gateway",
    /// `The federation as a list`
    TopologyListLabel = "topology-list-label",
    /// `The federation as a map`
    TopologyMapLabel = "topology-map-label",
    /// `median … ms`
    TopologyMedian = "topology-median",
    /// `No map is shown while the gateway does not answer, so a stale one …`
    TopologyNoStale = "topology-no-stale",
    /// `Open its page`
    TopologyOpenPage = "topology-open-page",
    /// `{ $count ->`
    TopologyOrgLine = "topology-org-line",
    /// `Organisations`
    TopologyOrganisations = "topology-organisations",
    /// `and their nodes`
    TopologyOrganisationsLine = "topology-organisations-line",
    /// `it answered with a 5xx, or slower than half the budget`
    TopologyRuleDegraded = "topology-rule-degraded",
    /// `no probe has run yet: a state, never a value`
    TopologyRuleNotChecked = "topology-rule-not-checked",
    /// `An organisation, a service group and the branch into each show the …`
    TopologyRuleNote = "topology-rule-note",
    /// `excluded, or its organisation excluded: asked nothing`
    TopologyRuleOff = "topology-rule-off",
    /// `no answer within the per-node budget`
    TopologyRuleUnreachable = "topology-rule-unreachable",
    /// `the last probe got a status below 500 within half the per-node budget`
    TopologyRuleUp = "topology-rule-up",
    /// `Services`
    TopologyServices = "topology-services",
    /// `that the gateway asks`
    TopologyServicesLine = "topology-services-line",
    /// `The gateway, the services it asks and the organisations with their …`
    TopologySubtitle = "topology-subtitle",
}
