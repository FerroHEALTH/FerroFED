// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console member a form describes, read and checked, and merged onto
//! the member the newest revision holds of its `node_id`.
//!
//! A member's Query API endpoints (`openehr-rest-query`) and its FHIR
//! endpoints (`hl7-fhir-rest`, the exchange-format endpoints) are each one
//! field of the form, one endpoint after another, separated by a
//! semicolon. Every refusal names the field, never what it holds.

use ferrofed_registry::console::{
    ConsoleEndpoint, ConsoleMember, ConsoleNode, ConsoleOrganisation,
};
use ferrofed_registry::id::{EndpointId, NodeId, OrganisationId, SystemId};
use ferrofed_registry::snapshot::{ConnectionType, EndpointStatus};

use crate::configure::act::server::{given, invalid};
use crate::configure::model::ConfigForm;
use crate::views::model::Refusal;

/// The member the form describes.
///
/// # Errors
/// Returns [`Refusal::Invalid`] for an id, a `system_id` or an endpoint that
/// does not read, and for a member with no Query API endpoint.
pub fn member(form: &ConfigForm) -> Result<ConsoleMember, Refusal> {
    let id_form = "1 to 64 letters, digits, '.', '-' or '_', starting with a letter or digit";
    let organisation =
        OrganisationId::new(form.organisation.trim()).map_err(|_bad| Refusal::Invalid {
            reason: format!("The organisation id is not {id_form}."),
        })?;
    let node = NodeId::new(form.node.trim()).map_err(|_bad| Refusal::Invalid {
        reason: format!("The node_id is not {id_form}."),
    })?;
    let system_id = SystemId::new(form.system_id.trim())
        .map_err(|_bad| invalid("The system_id is not an openEHR UID."))?;
    let mut endpoints = Vec::new();
    for line in lines(&form.endpoints) {
        endpoints.push(endpoint(line)?);
    }
    if endpoints.is_empty() {
        return Err(invalid("Give at least one Query API endpoint."));
    }
    for line in lines(&form.fhir_endpoints) {
        endpoints.push(fhir_endpoint(line)?);
    }
    Ok(ConsoleMember {
        organisation: ConsoleOrganisation {
            id: organisation,
            name: given(&form.organisation_name).map(str::to_owned),
        },
        node: ConsoleNode {
            id: node,
            system_id,
            product: given(&form.product).map(str::to_owned),
            version: given(&form.product_version).map(str::to_owned),
        },
        endpoints,
    })
}

/// The endpoints `field` names, one per semicolon, empty ones left out.
fn lines(field: &str) -> impl Iterator<Item = &str> {
    field
        .split(';')
        .map(str::trim)
        .filter(|line| !line.is_empty())
}

/// The `endpoint_id` `id` names.
fn endpoint_id(id: &str) -> Result<EndpointId, Refusal> {
    EndpointId::new(id)
        .map_err(|_bad| invalid("An endpoint_id is not 1 to 64 letters, digits, '.', '-' or '_'."))
}

/// One Query API endpoint: its `endpoint_id`, its base URL and
/// optionally `suspended`.
fn endpoint(line: &str) -> Result<ConsoleEndpoint, Refusal> {
    let shape =
        "Each endpoint is an endpoint_id and a base URL, then suspended if it is out of service.";
    let mut words = line.split_whitespace();
    let (Some(id), Some(url)) = (words.next(), words.next()) else {
        return Err(invalid(shape));
    };
    let status = match words.next() {
        None => EndpointStatus::Active,
        Some("suspended") => EndpointStatus::Suspended,
        Some(_) => return Err(invalid(shape)),
    };
    if words.next().is_some() {
        return Err(invalid(shape));
    }
    Ok(ConsoleEndpoint {
        id: endpoint_id(id)?,
        url: url.to_owned(),
        connection_type: ConnectionType::OpenehrRestQuery,
        status,
        consent_refusal_codes: Vec::new(),
        categories: Vec::new(),
        profiles: Vec::new(),
        ehr_id_system: None,
    })
}

/// One FHIR endpoint: its `endpoint_id` and base URL, then `system=`, the
/// identifier system it is told the `ehr_id` in, `categories=` and
/// `profiles=`, each a comma-separated list, and optionally `suspended`.
fn fhir_endpoint(line: &str) -> Result<ConsoleEndpoint, Refusal> {
    let shape = "Each FHIR endpoint is an endpoint_id and a base URL, then system=, categories= and profiles=, then suspended if it is out of service.";
    let mut words = line.split_whitespace();
    let (Some(id), Some(url)) = (words.next(), words.next()) else {
        return Err(invalid(shape));
    };
    let (mut system, mut categories, mut profiles) = (None, None, None);
    let mut status = EndpointStatus::Active;
    for word in words {
        let slot = match word.split_once('=') {
            Some(("system", value)) => (&mut system, value),
            Some(("categories", value)) => (&mut categories, value),
            Some(("profiles", value)) => (&mut profiles, value),
            None if word == "suspended" && status == EndpointStatus::Active => {
                status = EndpointStatus::Suspended;
                continue;
            }
            _ => return Err(invalid(shape)),
        };
        if slot.0.replace(slot.1).is_some() {
            return Err(invalid(shape));
        }
    }
    let (Some(system), Some(categories), Some(profiles)) = (system, categories, profiles) else {
        return Err(invalid(shape));
    };
    if system.is_empty() {
        return Err(invalid(
            "A FHIR endpoint names the identifier system after system=.",
        ));
    }
    Ok(ConsoleEndpoint {
        id: endpoint_id(id)?,
        url: url.to_owned(),
        connection_type: ConnectionType::Hl7FhirRest,
        status,
        consent_refusal_codes: Vec::new(),
        categories: listed(categories, "categories")?,
        profiles: listed(profiles, "profiles")?,
        ehr_id_system: Some(system.to_owned()),
    })
}

/// The comma-separated values of the FHIR endpoint key `key`, at least one
/// and none empty.
fn listed(values: &str, key: &str) -> Result<Vec<String>, Refusal> {
    let listed: Vec<String> = values.split(',').map(str::to_owned).collect();
    if listed.iter().any(String::is_empty) {
        return Err(Refusal::Invalid {
            reason: format!(
                "A FHIR endpoint names one or more {key}, separated by commas, after {key}=."
            ),
        });
    }
    Ok(listed)
}

/// Returns `form`, the member the page's form gives, merged onto `held`.
///
/// `held` is the member the newest revision holds of its `node_id`: each
/// endpoint the form names keeps every key the form does not show of the
/// held endpoint of its `endpoint_id` and connection type (a Query API
/// endpoint its consent refusal codes). An endpoint the form does not name
/// is left out, a FHIR endpoint among them.
#[must_use]
pub fn merged(mut form: ConsoleMember, held: Option<&ConsoleMember>) -> ConsoleMember {
    let Some(held) = held else {
        return form;
    };
    for endpoint in &mut form.endpoints {
        let kept = held.endpoints.iter().find(|kept| {
            kept.id == endpoint.id && kept.connection_type == endpoint.connection_type
        });
        if let Some(kept) = kept {
            let shown = std::mem::replace(endpoint, kept.clone());
            endpoint.url = shown.url;
            endpoint.status = shown.status;
            if shown.connection_type == ConnectionType::Hl7FhirRest {
                endpoint.categories = shown.categories;
                endpoint.profiles = shown.profiles;
                endpoint.ehr_id_system = shown.ehr_id_system;
            }
        }
    }
    form
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The FHIR endpoint the line `line` names, or the refusal's reason.
    fn read(line: &str) -> Result<ConsoleEndpoint, String> {
        fhir_endpoint(line).map_err(|refusal| match refusal {
            Refusal::Invalid { reason } => reason,
            other => format!("{other:?}"),
        })
    }

    #[test]
    #[expect(clippy::panic_in_result_fn, reason = "test assertions")]
    fn a_fhir_endpoint_reads_its_keys_in_any_order() -> Result<(), String> {
        let endpoint = read(
            "node-c-fhir https://cdr-c.example.org/fhir profiles=http://example.org/ps,http://example.org/ps2 suspended system=urn:oid:2.999.1 categories=Patient-Summaries",
        )?;
        assert_eq!("node-c-fhir", endpoint.id.as_str());
        assert_eq!(ConnectionType::Hl7FhirRest, endpoint.connection_type);
        assert_eq!(EndpointStatus::Suspended, endpoint.status);
        assert_eq!(vec![String::from("Patient-Summaries")], endpoint.categories);
        assert_eq!(2, endpoint.profiles.len());
        assert_eq!(Some("urn:oid:2.999.1"), endpoint.ehr_id_system.as_deref());
        Ok(())
    }

    #[test]
    fn a_fhir_endpoint_missing_or_repeating_a_key_is_refused() {
        for line in [
            "node-c-fhir https://x.example.org/fhir categories=A profiles=p",
            "node-c-fhir https://x.example.org/fhir system=s profiles=p",
            "node-c-fhir https://x.example.org/fhir system=s categories=A",
            "node-c-fhir https://x.example.org/fhir system= categories=A profiles=p",
            "node-c-fhir https://x.example.org/fhir system=s categories=A, profiles=p",
            "node-c-fhir https://x.example.org/fhir system=s categories=A profiles=p system=t",
            "node-c-fhir https://x.example.org/fhir system=s categories=A profiles=p suspended suspended",
            "node-c-fhir https://x.example.org/fhir system=s categories=A profiles=p colour=blue",
            "node-c-fhir",
            "node c https://x.example.org/fhir system=s categories=A profiles=p",
        ] {
            assert!(read(line).is_err(), "{line}");
        }
    }
}
