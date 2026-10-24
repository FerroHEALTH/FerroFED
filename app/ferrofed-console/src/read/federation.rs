// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The federation as the console draws it: the organisations, their nodes
//! and each node's endpoints, and the services the gateway asks.
//!
//! It is read from the gateway's self-description (`OPTIONS {base}/`,
//! §7a.2, N30), which lists every member endpoint with its managing
//! organisation (N20), its node and its membership status, and from the
//! last state the gateway observed of each endpoint and service
//! (`GET {base}/operator/dependencies`). An organisation's name comes from
//! the configuration the console manages where it holds one; elsewhere the
//! organisation is named by its id. Every id here is a routing id of the
//! registry, never a patient identifier (§5.4.1, N33), so a page path may
//! carry it.

use std::collections::BTreeMap;

use ferrofed_registry::console::Owner;
use ferrofed_registry::health::{DependencyReport, Indication, Observed};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::flow::FlowState;
use ferrohealth_ui::view::state::Mark;
use openehr_federation::options::{MemberEndpoint, OptionsRoot};

use crate::locale::{Msg, tr};
use crate::read::{Stop, signed_in};

/// The state of an endpoint, a node, an organisation or a service, as a
/// shape, a colour and a word (FerroUI family rule 12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Health {
    /// It answered the last request: a filled dot.
    Reachable,
    /// No request has reached it yet: a hollow ring.
    NotProbed,
    /// It was reached and failed, or work waits for it: a diamond.
    Degraded,
    /// It did not answer: a cross.
    Unreachable,
    /// Its membership is not active, so the gateway asks it nothing: a
    /// muted row.
    Excluded,
}

impl Health {
    /// The state the gateway last observed, `None` for none observed.
    #[must_use]
    pub const fn of(observed: Option<Observed>) -> Self {
        match observed {
            Some(Observed::Up) => Self::Reachable,
            Some(Observed::Failing | Observed::Degraded) => Self::Degraded,
            Some(Observed::Down) => Self::Unreachable,
            Some(Observed::Unknown) | None => Self::NotProbed,
        }
    }

    /// The state as the topology and the navigation draw it.
    #[must_use]
    pub const fn flow(self) -> FlowState {
        match self {
            Self::Reachable => FlowState::Up,
            Self::NotProbed => FlowState::NotChecked,
            Self::Degraded => FlowState::Degraded,
            Self::Unreachable => FlowState::Unreachable,
            Self::Excluded => FlowState::Off,
        }
    }

    /// The state mark (F12) its word is drawn beside.
    #[must_use]
    pub fn mark(self) -> Mark {
        Mark::from(self.flow())
    }

    /// The tone of its word.
    #[must_use]
    pub const fn tone(self) -> Tone {
        match self {
            Self::Reachable => Tone::Ok,
            Self::NotProbed | Self::Excluded => Tone::Neutral,
            Self::Degraded => Tone::Warn,
            Self::Unreachable => Tone::Bad,
        }
    }

    /// Its word, with a capital.
    #[must_use]
    pub fn word(self) -> String {
        tr(match self {
            Self::Reachable => Msg::StateReachable,
            Self::NotProbed => Msg::StateNotProbed,
            Self::Degraded => Msg::StateDegraded,
            Self::Unreachable => Msg::StateUnreachable,
            Self::Excluded => Msg::StateExcluded,
        })
    }

    /// Whether it asks for the operator's attention.
    #[must_use]
    pub const fn is_problem(self) -> bool {
        matches!(self, Self::Degraded | Self::Unreachable)
    }

    /// How bad it is, for the worst of several: a problem outranks a
    /// healthy state, and an excluded part counts least.
    const fn rank(self) -> u8 {
        match self {
            Self::Excluded => 0,
            Self::Reachable => 1,
            Self::NotProbed => 2,
            Self::Degraded => 3,
            Self::Unreachable => 4,
        }
    }

    /// The worst of `states`, `Excluded` for none.
    #[must_use]
    pub fn worst(states: impl IntoIterator<Item = Self>) -> Self {
        states
            .into_iter()
            .max_by_key(|state| state.rank())
            .unwrap_or(Self::Excluded)
    }
}

/// One member endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// The `endpoint_id`.
    pub id: String,
    /// The CDR base URL, when the self-description names it.
    pub url: Option<String>,
    /// The membership status as the self-description writes it.
    pub status: String,
    /// Its state.
    pub health: Health,
    /// The median latency over recent requests, in milliseconds.
    pub latency_ms_p50: Option<u64>,
    /// The 95th percentile latency over the same requests, in milliseconds.
    pub latency_ms_p95: Option<u64>,
}

/// One node and its endpoints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// The `node_id`, or the `endpoint_id` of an endpoint that names no
    /// node.
    pub id: String,
    /// The node's `system_id`, where known.
    pub system_id: Option<String>,
    /// The CDR product and version, where known.
    pub product: Option<String>,
    /// Its endpoints, in the order the self-description lists them.
    pub endpoints: Vec<Endpoint>,
    /// What owns the node's member entry, where the configuration the
    /// console reads says so: the file, a directory or the console.
    pub owner: Option<Owner>,
}

impl Node {
    /// Its state: the worst of its endpoints that take part, `Excluded`
    /// when none does.
    #[must_use]
    pub fn health(&self) -> Health {
        Health::worst(self.endpoints.iter().map(|endpoint| endpoint.health))
    }

    /// The lowest median latency of its endpoints, in milliseconds.
    #[must_use]
    pub fn latency_ms_p50(&self) -> Option<u64> {
        self.fastest().and_then(|endpoint| endpoint.latency_ms_p50)
    }

    /// The 95th percentile latency of the endpoint with the lowest median,
    /// in milliseconds, so it pairs with [`Node::latency_ms_p50`].
    #[must_use]
    pub fn latency_ms_p95(&self) -> Option<u64> {
        self.fastest().and_then(|endpoint| endpoint.latency_ms_p95)
    }

    /// The endpoint with the lowest median latency, the first of a tie.
    fn fastest(&self) -> Option<&Endpoint> {
        self.endpoints
            .iter()
            .filter(|endpoint| endpoint.latency_ms_p50.is_some())
            .min_by_key(|endpoint| endpoint.latency_ms_p50)
    }

    /// Whether it has an id its page is linked by.
    #[must_use]
    pub fn routable(&self) -> bool {
        !self.id.trim().is_empty()
    }
}

/// One organisation and its nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Organisation {
    /// The organisation's id.
    pub id: String,
    /// Its name, where the configuration the console manages holds one.
    pub name: Option<String>,
    /// Its nodes, by id.
    pub nodes: Vec<Node>,
}

impl Organisation {
    /// The words it is shown by: its name, or its id.
    #[must_use]
    pub fn shown(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }

    /// Its state: the worst of its nodes.
    #[must_use]
    pub fn health(&self) -> Health {
        Health::worst(self.nodes.iter().map(Node::health))
    }

    /// How many endpoints its nodes hold.
    #[must_use]
    pub fn endpoint_count(&self) -> usize {
        self.nodes.iter().map(|node| node.endpoints.len()).sum()
    }

    /// Whether it has an id its page is linked by.
    #[must_use]
    pub fn routable(&self) -> bool {
        !self.id.trim().is_empty()
    }

    /// The path of its page, its id percent-encoded as one segment.
    #[must_use]
    pub fn href(&self) -> String {
        format!(
            "{}/{}",
            crate::app::ORGANISATIONS,
            crate::pages::stored::encoded(&self.id)
        )
    }

    /// The path of the page of its node `node`.
    #[must_use]
    pub fn node_href(&self, node: &Node) -> String {
        format!(
            "{}/{}",
            self.href(),
            crate::pages::stored::encoded(&node.id)
        )
    }
}

/// The kind of a service the gateway asks, which groups the services.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ServiceGroup {
    /// Who the patient is: the resolver, the demographics service and the
    /// identity feed.
    Identity,
    /// Where the patient's records are: the localizer.
    Localization,
    /// Whether the patient consents: the consent pre-filter.
    Consent,
    /// Where the members are: the care services directory.
    Directory,
    /// Where the audit records go.
    Audit,
}

impl ServiceGroup {
    /// The group's heading.
    #[must_use]
    pub fn title(self) -> String {
        tr(match self {
            Self::Identity => Msg::GroupIdentity,
            Self::Localization => Msg::GroupLocalization,
            Self::Consent => Msg::GroupConsent,
            Self::Directory => Msg::GroupDirectory,
            Self::Audit => Msg::GroupAudit,
        })
    }

    /// The group's key, which its topology card and its fold are named by.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Localization => "localization",
            Self::Consent => "consent",
            Self::Directory => "directory",
            Self::Audit => "audit",
        }
    }
}

/// One service the gateway asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    /// Its key in the dependency report, such as `resolver`.
    pub key: String,
    /// Its group.
    pub group: ServiceGroup,
    /// Its state.
    pub health: Health,
    /// The class of the fault the gateway names beside it, such as
    /// `refused-credentials`, if any.
    pub fault: Option<String>,
}

impl Service {
    /// Its name in plain words.
    #[must_use]
    pub fn name(&self) -> String {
        service_name(&self.key)
    }

    /// What it does for the federation, in plain words.
    #[must_use]
    pub fn role(&self) -> String {
        tr(match self.key.as_str() {
            "resolver" => Msg::RoleResolver,
            "demographics" => Msg::RoleDemographics,
            "identity_registry" => Msg::RoleIdentityRegistry,
            "localizer" => Msg::RoleLocalizer,
            "consent" => Msg::RoleConsent,
            "directory" => Msg::RoleDirectory,
            "audit_repository" => Msg::RoleAuditRepository,
            "audit_feed" => Msg::RoleAuditFeed,
            _ => Msg::RoleOther,
        })
    }

    /// The path of its page.
    #[must_use]
    pub fn href(&self) -> String {
        format!("{}/{}", crate::app::SERVICES, self.key)
    }
}

/// The name of the service `key` in plain words, the key itself for one
/// the console does not know.
#[must_use]
pub fn service_name(key: &str) -> String {
    let msg = match key {
        "resolver" => Msg::ServiceResolver,
        "demographics" => Msg::ServiceDemographics,
        "identity_registry" => Msg::ServiceIdentityRegistry,
        "localizer" => Msg::ServiceLocalizer,
        "consent" => Msg::ServiceConsent,
        "directory" => Msg::ServiceDirectory,
        "audit_repository" => Msg::ServiceAuditRepository,
        "audit_feed" => Msg::ServiceAuditFeed,
        _ => return key.to_owned(),
    };
    tr(msg)
}

/// The group of the service `key`.
fn group_of(key: &str) -> ServiceGroup {
    match key {
        "localizer" => ServiceGroup::Localization,
        "consent" => ServiceGroup::Consent,
        "directory" => ServiceGroup::Directory,
        "audit_repository" | "audit_feed" => ServiceGroup::Audit,
        _ => ServiceGroup::Identity,
    }
}

/// What the configuration the console reads says of the members: each
/// organisation's name and each node's owner, by id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Known {
    /// The organisations' names, by organisation id.
    pub names: BTreeMap<String, String>,
    /// The nodes' owners, by node id.
    pub owners: BTreeMap<String, Owner>,
}

/// The endpoint member in which the gateway names the 95th percentile
/// latency beside `latency_ms_p50`.
// NOTE: §7a.2 has a gateway name the statistic in the field; the gateway writes it in the open
// endpoint object of options-root.schema.json.
const LATENCY_MS_P95: &str = "latency_ms_p95";

/// The 95th percentile latency `endpoint` carries, or `None` where it names
/// none or names one that is no whole number of milliseconds.
pub(crate) fn p95(endpoint: &MemberEndpoint) -> Option<u64> {
    // NOTE: an unreadable statistic is shown as not measured; it is advisory and the page
    // never guesses one.
    endpoint
        .extra
        .get(LATENCY_MS_P95)
        .and_then(|raw| serde_json::from_str(raw.get()).ok())
}

/// The federation the console draws.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Federation {
    /// The federation's id, as the self-description names it.
    pub id: String,
    /// The organisations, by the words they are shown by.
    pub organisations: Vec<Organisation>,
    /// The services, by group and key.
    pub services: Vec<Service>,
    /// The replica whose observations the states are, as the dependency
    /// report names it.
    pub replica: Option<String>,
}

impl Federation {
    /// Builds the federation of `description` and `dependencies`, each
    /// organisation named and each node owned as `known` says.
    #[must_use]
    pub fn of(description: &OptionsRoot, dependencies: &DependencyReport, known: &Known) -> Self {
        let mut organisations: BTreeMap<String, BTreeMap<String, Node>> = BTreeMap::new();
        for endpoint in &description.endpoints {
            let id = endpoint.id.to_string();
            let active = endpoint.status.as_str() == "active";
            let health = if active {
                Health::of(dependencies.endpoints.get(&id).copied())
            } else {
                Health::Excluded
            };
            let node_id = endpoint.node_id.clone().unwrap_or_else(|| id.clone());
            let nodes = organisations
                .entry(endpoint.organisation.clone())
                .or_default();
            let node = nodes.entry(node_id.clone()).or_insert_with(|| Node {
                id: node_id.clone(),
                system_id: endpoint.system_id.clone(),
                product: match (&endpoint.product, &endpoint.version) {
                    (Some(product), Some(version)) => Some(format!("{product} {version}")),
                    (Some(product), None) => Some(product.clone()),
                    (None, _) => None,
                },
                endpoints: Vec::new(),
                owner: known.owners.get(&node_id).copied(),
            });
            node.endpoints.push(Endpoint {
                id,
                url: endpoint.url.as_ref().map(ToString::to_string),
                status: endpoint.status.as_str().to_owned(),
                health,
                latency_ms_p50: endpoint.latency_ms_p50,
                latency_ms_p95: p95(endpoint),
            });
        }
        let mut organisations: Vec<Organisation> = organisations
            .into_iter()
            .map(|(id, nodes)| Organisation {
                name: known.names.get(&id).cloned(),
                id,
                nodes: nodes.into_values().collect(),
            })
            .collect();
        organisations.sort_by_key(|organisation| organisation.shown().to_lowercase());
        Self {
            id: description.federation.id.to_string(),
            organisations,
            services: services(dependencies),
            replica: dependencies.replica.clone(),
        }
    }

    /// Every node with its organisation.
    pub fn nodes(&self) -> impl Iterator<Item = (&Organisation, &Node)> {
        self.organisations.iter().flat_map(|organisation| {
            organisation
                .nodes
                .iter()
                .map(move |node| (organisation, node))
        })
    }

    /// The organisation `id`.
    #[must_use]
    pub fn organisation(&self, id: &str) -> Option<&Organisation> {
        self.organisations
            .iter()
            .find(|organisation| organisation.id == id)
    }

    /// The node `id` and its organisation.
    #[must_use]
    pub fn node(&self, id: &str) -> Option<(&Organisation, &Node)> {
        self.nodes().find(|(_, node)| node.id == id)
    }

    /// The service `key`.
    #[must_use]
    pub fn service(&self, key: &str) -> Option<&Service> {
        self.services.iter().find(|service| service.key == key)
    }
}

/// The services of `dependencies`, each with the fault its report names
/// beside it, by group and key.
fn services(dependencies: &DependencyReport) -> Vec<Service> {
    let mut services: Vec<Service> = [
        ("resolver", dependencies.resolver),
        ("demographics", dependencies.demographics),
        ("localizer", dependencies.localizer),
        ("consent", dependencies.consent),
    ]
    .into_iter()
    .filter_map(|(key, observed)| {
        observed.map(|observed| Service {
            key: key.to_owned(),
            group: group_of(key),
            health: Health::of(Some(observed)),
            fault: None,
        })
    })
    .collect();
    for (key, indication) in &dependencies.bindings {
        if key.ends_with("_fault") {
            continue;
        }
        let (health, fault) = match indication {
            Indication::State(observed) => (Health::of(Some(*observed)), None),
            Indication::Fault(class) => (Health::Degraded, Some(class.clone())),
        };
        let fault = fault.or_else(
            || match dependencies.bindings.get(&format!("{key}_fault")) {
                Some(Indication::Fault(class)) => Some(class.clone()),
                _ => None,
            },
        );
        services.push(Service {
            key: key.clone(),
            group: group_of(key),
            health,
            fault,
        });
    }
    services.sort_by(|a, b| (a.group, &a.key).cmp(&(b.group, &b.key)));
    services
}

/// Reads the federation as the operator of the request.
///
/// It reads the self-description, the dependency report, and the
/// organisations' names from the configuration the console manages when it
/// is configured with an admin listener that answers.
///
/// # Errors
/// Returns the [`Stop`] of a missing session, or of the self-description
/// or the dependency report the gateway refused or did not answer.
pub async fn read() -> Result<Federation, Stop> {
    let (state, token) = signed_in()?;
    let gateway = state.gateway();
    let description = gateway.self_description(&token).await?;
    let dependencies = gateway.dependencies(&token).await?;
    let mut known = Known::default();
    if let Some(replica) = state.settings().admin.first()
        // NOTE: no specification governs this: our own design; a name is a courtesy, so a
        // configuration that cannot be read leaves each organisation named by its id.
        && let Ok(resources) = gateway.config_resources(replica, &token).await
    {
        for member in resources.members {
            known.owners.insert(member.node.to_string(), member.owner);
            if let Some(name) = member.member.and_then(|member| member.organisation.name) {
                known.names.insert(member.organisation.to_string(), name);
            }
        }
    }
    Ok(Federation::of(&description, &dependencies, &known))
}

#[cfg(test)]
mod tests {
    use super::{Endpoint, Health, Node};
    use ferrofed_registry::health::Observed;

    fn endpoint(id: &str, latency: Option<(u64, u64)>) -> Endpoint {
        Endpoint {
            id: id.to_owned(),
            url: None,
            status: "active".to_owned(),
            health: Health::Reachable,
            latency_ms_p50: latency.map(|(p50, _)| p50),
            latency_ms_p95: latency.map(|(_, p95)| p95),
        }
    }

    #[test]
    fn a_node_shows_the_pair_of_its_endpoint_with_the_lowest_median() {
        let node = Node {
            id: "node-a".to_owned(),
            system_id: None,
            product: None,
            endpoints: vec![
                endpoint("slow", Some((90, 120))),
                endpoint("unasked", None),
                endpoint("fast", Some((20, 3000))),
            ],
            owner: None,
        };
        assert_eq!(
            (Some(20), Some(3000)),
            (node.latency_ms_p50(), node.latency_ms_p95())
        );
        let unasked = Node {
            endpoints: vec![endpoint("unasked", None)],
            ..node
        };
        assert_eq!(
            (None, None),
            (unasked.latency_ms_p50(), unasked.latency_ms_p95())
        );
    }

    #[test]
    fn the_worst_state_wins_and_an_excluded_part_counts_least() {
        assert_eq!(
            Health::Degraded,
            Health::worst([Health::Reachable, Health::Degraded, Health::Excluded])
        );
        assert_eq!(
            Health::Unreachable,
            Health::worst([Health::Degraded, Health::Unreachable])
        );
        assert_eq!(Health::Excluded, Health::worst([Health::Excluded]));
        assert_eq!(Health::Excluded, Health::worst([]));
        assert_eq!(Health::NotProbed, Health::of(None));
        assert_eq!(Health::Degraded, Health::of(Some(Observed::Failing)));
    }
}
