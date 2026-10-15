// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console member a form describes, read and checked, and merged onto
//! the member the newest revision holds of its `node_id`.
//!
//! A member's Query API endpoints (`openehr-rest-query`) and its FHIR
//! endpoints (`hl7-fhir-rest`, the exchange-format endpoints) are each one
//! field of the form, one endpoint after another, separated by a
//! semicolon; a semicolon inside an endpoint is written `%3B`
//! ([`escaped`]). Every refusal names the field and the endpoint's
//! position, never what it holds.
//!
//! The fields stay `<input>` elements: the server renders their value into
//! an attribute, which Leptos escapes, never into a `<textarea>`, whose
//! children it leaves unescaped.

use ferrofed_registry::console::{
    ConsoleEndpoint, ConsoleMember, ConsoleNode, ConsoleOrganisation,
};
use ferrofed_registry::id::{EndpointId, NodeId, OrganisationId, SystemId};
use ferrofed_registry::snapshot::{ConnectionType, EndpointStatus};

use crate::configure::act::server::{given, invalid};
use crate::configure::model::ConfigForm;
use crate::views::model::Refusal;

/// How the form that gave a member was drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shown {
    /// Whether it is the form that adds a member, which refuses a `node_id`
    /// the gateway already holds.
    pub adding: bool,
    /// Whether the form showed the FHIR endpoints field, by its `fhir_shown`
    /// marker; a form without it keeps the held FHIR endpoints whole.
    pub fhir: bool,
}

/// The member the form describes, and how its form was drawn.
///
/// # Errors
/// Returns [`Refusal::Invalid`] for an id, a `system_id` or an endpoint that
/// does not read, and for a member with no Query API endpoint.
pub fn member(form: &ConfigForm) -> Result<(ConsoleMember, Shown), Refusal> {
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
    for (position, line) in lines(&form.endpoints) {
        endpoints.push(endpoint(&line, position)?);
    }
    if endpoints.is_empty() {
        return Err(invalid("Give at least one Query API endpoint."));
    }
    for (position, line) in lines(&form.fhir_endpoints) {
        endpoints.push(fhir_endpoint(&line, position)?);
    }
    let member = ConsoleMember {
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
    };
    let shown = Shown {
        adding: form.action == "add-member",
        fhir: given(&form.fhir_shown).is_some(),
    };
    Ok((member, shown))
}

/// The endpoints `field` names, one per semicolon, empty ones left out,
/// each read back with [`unescaped`] and with its position from 1.
fn lines(field: &str) -> impl Iterator<Item = (usize, String)> {
    field
        .split(';')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .zip(1_usize..)
        .map(|(line, position)| (position, unescaped(line)))
}

/// Returns `endpoint` as an endpoints field writes it, so the semicolon
/// that separates endpoints never occurs inside one.
///
/// A `;` is written `%3B`, and a `%` followed by `3B` after any number of
/// `25` gains one `25` after it, so a save reads back `endpoint`
/// exactly and any other percent-encoding passes as it stands.
#[must_use]
pub fn escaped(endpoint: &str) -> String {
    let mut written = String::with_capacity(endpoint.len());
    let mut chars = endpoint.chars();
    while let Some(next) = chars.next() {
        match next {
            ';' => written.push_str("%3B"),
            '%' if escapes_semicolon(chars.as_str()) => written.push_str("%25"),
            other => written.push(other),
        }
    }
    written
}

/// Returns the endpoint `written` stands for, undoing [`escaped`] once.
fn unescaped(written: &str) -> String {
    let mut endpoint = String::with_capacity(written.len());
    let mut chars = written.chars();
    while let Some(next) = chars.next() {
        let rest = chars.as_str();
        if next == '%' {
            if let Some(after) = rest.strip_prefix("3B").or_else(|| rest.strip_prefix("3b")) {
                endpoint.push(';');
                chars = after.chars();
                continue;
            }
            if let Some(after) = rest.strip_prefix("25")
                && escapes_semicolon(after)
            {
                endpoint.push('%');
                chars = after.chars();
                continue;
            }
        }
        endpoint.push(next);
    }
    endpoint
}

/// Whether `rest`, the text after a `%`, is any number of `25` and then
/// `3B` in either case.
fn escapes_semicolon(rest: &str) -> bool {
    let mut rest = rest;
    while let Some(after) = rest.strip_prefix("25") {
        rest = after;
    }
    rest.starts_with("3B") || rest.starts_with("3b")
}

/// The `endpoint_id` `id` of the `kind` endpoint at `position`.
fn endpoint_id(id: &str, kind: &str, position: usize) -> Result<EndpointId, Refusal> {
    EndpointId::new(id).map_err(|_bad| Refusal::Invalid {
        reason: format!(
            "The endpoint_id of {kind} endpoint {position} is not 1 to 64 letters, digits, '.', '-' or '_'."
        ),
    })
}

/// The base URL `url` of the `kind` endpoint at `position`: an absolute
/// `http` or `https` URL with a host.
fn base_url(url: &str, kind: &str, position: usize) -> Result<String, Refusal> {
    match url::Url::parse(url) {
        Ok(parsed)
            if matches!(parsed.scheme(), "http" | "https") && parsed.host_str().is_some() =>
        {
            Ok(url.to_owned())
        }
        _ => Err(Refusal::Invalid {
            reason: format!(
                "The base URL of {kind} endpoint {position} is not an absolute http or https URL."
            ),
        }),
    }
}

/// One Query API endpoint, at `position`: its `endpoint_id`, its base URL
/// and optionally `suspended`.
fn endpoint(line: &str, position: usize) -> Result<ConsoleEndpoint, Refusal> {
    let shape = || Refusal::Invalid {
        reason: format!(
            "Query API endpoint {position} is not an endpoint_id and a base URL, then suspended if it is out of service."
        ),
    };
    let mut words = line.split_whitespace();
    let (Some(id), Some(url)) = (words.next(), words.next()) else {
        return Err(shape());
    };
    let status = match words.next() {
        None => EndpointStatus::Active,
        Some("suspended") => EndpointStatus::Suspended,
        Some(_) => return Err(shape()),
    };
    if words.next().is_some() {
        return Err(shape());
    }
    Ok(ConsoleEndpoint {
        id: endpoint_id(id, "Query API", position)?,
        url: base_url(url, "Query API", position)?,
        connection_type: ConnectionType::OpenehrRestQuery,
        status,
        consent_refusal_codes: Vec::new(),
        categories: Vec::new(),
        profiles: Vec::new(),
        ehr_id_system: None,
    })
}

/// One FHIR endpoint, at `position`: its `endpoint_id` and base URL, then
/// `system=`, the identifier system it is told the `ehr_id` in,
/// `categories=` and `profiles=`, each a comma-separated list, and
/// optionally `suspended`.
fn fhir_endpoint(line: &str, position: usize) -> Result<ConsoleEndpoint, Refusal> {
    let shape = || Refusal::Invalid {
        reason: format!(
            "FHIR endpoint {position} is not an endpoint_id and a base URL, then system=, categories= and profiles=, then suspended if it is out of service."
        ),
    };
    let mut words = line.split_whitespace();
    let (Some(id), Some(url)) = (words.next(), words.next()) else {
        return Err(shape());
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
            _ => return Err(shape()),
        };
        if slot.0.replace(slot.1).is_some() {
            return Err(shape());
        }
    }
    let (Some(system), Some(categories), Some(profiles)) = (system, categories, profiles) else {
        return Err(shape());
    };
    if system.is_empty() {
        return Err(Refusal::Invalid {
            reason: format!("FHIR endpoint {position} names no identifier system after system=."),
        });
    }
    Ok(ConsoleEndpoint {
        id: endpoint_id(id, "FHIR", position)?,
        url: base_url(url, "FHIR", position)?,
        connection_type: ConnectionType::Hl7FhirRest,
        status,
        consent_refusal_codes: Vec::new(),
        categories: listed(categories, "categories", position)?,
        profiles: listed(profiles, "profiles", position)?,
        ehr_id_system: Some(system.to_owned()),
    })
}

/// The comma-separated values of the key `key` of the FHIR endpoint at
/// `position`, at least one and none empty.
fn listed(values: &str, key: &str, position: usize) -> Result<Vec<String>, Refusal> {
    let listed: Vec<String> = values.split(',').map(str::to_owned).collect();
    if listed.iter().any(String::is_empty) {
        return Err(Refusal::Invalid {
            reason: format!(
                "FHIR endpoint {position} names one or more {key}, separated by commas, after {key}=."
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
/// endpoint its consent refusal codes). A held FHIR endpoint the form does
/// not name is left out when the form showed the FHIR endpoints, and kept
/// whole when it did not (`shown_fhir`).
#[must_use]
pub fn merged(
    mut form: ConsoleMember,
    held: Option<&ConsoleMember>,
    shown_fhir: bool,
) -> ConsoleMember {
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
    if !shown_fhir {
        let natives: Vec<ConsoleEndpoint> = held
            .endpoints
            .iter()
            .filter(|kept| kept.connection_type == ConnectionType::Hl7FhirRest)
            .filter(|kept| !form.endpoints.iter().any(|named| named.id == kept.id))
            .cloned()
            .collect();
        form.endpoints.extend(natives);
    }
    form
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The FHIR endpoint the line `line` names, or the refusal's reason.
    fn read(line: &str) -> Result<ConsoleEndpoint, String> {
        fhir_endpoint(line, 2).map_err(|refusal| match refusal {
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
    fn a_fhir_endpoint_missing_or_repeating_a_key_is_refused_by_its_position() {
        for line in [
            "node-c-fhir https://x.example.org/fhir categories=A profiles=p",
            "node-c-fhir https://x.example.org/fhir system=s profiles=p",
            "node-c-fhir https://x.example.org/fhir system=s categories=A",
            "node-c-fhir https://x.example.org/fhir system= categories=A profiles=p",
            "node-c-fhir https://x.example.org/fhir system=s categories=A, profiles=p",
            "node-c-fhir https://x.example.org/fhir system=s categories=A profiles=p system=t",
            "node-c-fhir https://x.example.org/fhir system=s categories=A profiles=p suspended suspended",
            "node-c-fhir https://x.example.org/fhir system=s categories=A profiles=p colour=blue",
            "node-c-fhir x.example.org/fhir system=s categories=A profiles=p",
            "node-c-fhir",
            "node c https://x.example.org/fhir system=s categories=A profiles=p",
        ] {
            let refused = read(line).err();
            assert!(
                refused
                    .as_deref()
                    .is_some_and(|reason| reason.contains("FHIR endpoint 2")),
                "{line}: {refused:?}"
            );
        }
    }

    #[test]
    fn an_endpoint_escaped_for_the_field_reads_back_whole() {
        for held in [
            "node-c-pub https://cdr-c.example.org/openehr;v=1/x;y",
            "node-c-pub https://cdr-c.example.org/a%3Bb%3bc%253Bd%2525%3B%20e",
            "node-c-pub https://cdr-c.example.org/%25%2%3%",
            "node-c-fhir https://x.example.org/fhir;r4 system=urn:a;b categories=A profiles=http://p.example.org/x;1",
        ] {
            let written = escaped(held);
            assert!(!written.contains(';'), "{written}");
            let read: Vec<(usize, String)> = lines(&format!("{written}; {written}")).collect();
            assert_eq!(
                vec![(1, held.to_owned()), (2, held.to_owned())],
                read,
                "{written}"
            );
        }
        assert_eq!(
            "https://x.example.org/a%20b",
            unescaped("https://x.example.org/a%20b")
        );
    }

    #[test]
    fn a_query_endpoint_whose_url_is_not_absolute_http_is_refused() {
        for url in [
            "cdr.example.org/openehr",
            "ftp://cdr.example.org/openehr",
            "https://",
            "mailto:a@example.org",
        ] {
            let refused = endpoint(&format!("node-c-pub {url}"), 3).err();
            assert_eq!(
                Some(Refusal::Invalid {
                    reason: String::from(
                        "The base URL of Query API endpoint 3 is not an absolute http or https URL."
                    )
                }),
                refused,
                "{url}"
            );
        }
    }
}
