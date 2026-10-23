// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The messages of `locales/en/federation.ftl`.

messages! {
    /// `All nodes`
    AllNodes = "all-nodes",
    /// `Edit`
    EditNode = "edit-node",
    /// `{ $count ->`
    EndpointCount = "endpoint-count",
    /// `Audit`
    GroupAudit = "group-audit",
    /// `Consent`
    GroupConsent = "group-consent",
    /// `Directory`
    GroupDirectory = "group-directory",
    /// `Identity`
    GroupIdentity = "group-identity",
    /// `Localization`
    GroupLocalization = "group-localization",
    /// `New node`
    NewNode = "new-node",
    /// `New organisation`
    NewOrganisation = "new-organisation",
    /// `Facts`
    NodeFacts = "node-facts",
    /// `Health`
    NodeHealth = "node-health",
    /// `The last state the gateway observed of each endpoint.`
    NodeHealthLine = "node-health-line",
    /// `Node of …`
    NodeSubtitle = "node-subtitle",
    /// `No node matches.`
    NodesNoMatch = "nodes-no-match",
    /// `{ $count ->`
    NounEndpoints = "noun-endpoints",
    /// `{ $count ->`
    NounNodes = "noun-nodes",
    /// `{ $count ->`
    NounOrganisations = "noun-organisations",
    /// `{ $count ->`
    OrganisationCount = "organisation-count",
    /// `Facts`
    OrganisationFacts = "organisation-facts",
    /// `Organisation … · { $count ->`
    OrganisationSubtitle = "organisation-subtitle",
    /// `Add the first organisation`
    OrganisationsAddFirst = "organisations-add-first",
    /// `Find an organisation or a node`
    OrganisationsFind = "organisations-find",
    /// `No organisation matches.`
    OrganisationsNoMatch = "organisations-no-match",
    /// `No organisation takes part yet.`
    OrganisationsNone = "organisations-none",
    /// `The organisations in the federation and their nodes.`
    OrganisationsSubtitle = "organisations-subtitle",
    /// `Organisations`
    OrganisationsTitle = "organisations-title",
    /// `Receives the audit records of the identity transactions.`
    RoleAuditFeed = "role-audit-feed",
    /// `Receives the audit record of every transaction.`
    RoleAuditRepository = "role-audit-repository",
    /// `Asks whether a patient's records may be shared, before any node is …`
    RoleConsent = "role-consent",
    /// `Finds a patient's master identity before resolution.`
    RoleDemographics = "role-demographics",
    /// `Lists the organisations and their endpoints the registry is read from.`
    RoleDirectory = "role-directory",
    /// `Tells the gateway when a patient's identity changes.`
    RoleIdentityRegistry = "role-identity-registry",
    /// `Finds the nodes that hold a patient's records.`
    RoleLocalizer = "role-localizer",
    /// `A service a binding runs or records through.`
    RoleOther = "role-other",
    /// `Resolves a patient to each node's own EHR id.`
    RoleResolver = "role-resolver",
    /// `Learned`
    RouteLearned = "route-learned",
    /// `Its own system id`
    RouteOwn = "route-own",
    /// `Registered`
    RouteRegistered = "route-registered",
    /// `Withdrawn`
    RouteWithdrawn = "route-withdrawn",
    /// `The creating systems whose follow-ups go to this node.`
    RoutesLine = "routes-line",
    /// `No creating system routes to ….`
    RoutesNone = "routes-none",
    /// `Audit feed`
    ServiceAuditFeed = "service-audit-feed",
    /// `Audit record repository`
    ServiceAuditRepository = "service-audit-repository",
    /// `Consent pre-filter`
    ServiceConsent = "service-consent",
    /// `Demographics`
    ServiceDemographics = "service-demographics",
    /// `Care services directory`
    ServiceDirectory = "service-directory",
    /// `Health`
    ServiceHealth = "service-health",
    /// `The last state the replica observed; each replica observes its own.`
    ServiceHealthLine = "service-health-line",
    /// `Identity feed`
    ServiceIdentityRegistry = "service-identity-registry",
    /// `Localizer`
    ServiceLocalizer = "service-localizer",
    /// `Identity cross-reference`
    ServiceResolver = "service-resolver",
    /// `The gateway asks no service.`
    ServicesNone = "services-none",
    /// `The services the gateway asks, with the state the replica last …`
    ServicesSubtitle = "services-subtitle",
    /// `Services`
    ServicesTitle = "services-title",
    /// `Degraded`
    StateDegraded = "state-degraded",
    /// `Excluded`
    StateExcluded = "state-excluded",
    /// `Not probed`
    StateNotProbed = "state-not-probed",
    /// `Reachable`
    StateReachable = "state-reachable",
    /// `Unreachable`
    StateUnreachable = "state-unreachable",
}
