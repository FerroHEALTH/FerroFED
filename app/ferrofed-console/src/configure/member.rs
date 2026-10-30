// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console member a form describes, read and checked, and merged onto
//! the member the newest revision holds of its `node_id`.
//!
//! A member's Query API endpoints (`openehr-rest-query`) and its FHIR
//! endpoints (`hl7-fhir-rest`, the exchange-format endpoints) are rows of
//! the form, each field of each row its own input, so no value is ever
//! written as separated or escaped text. Every refusal names the field it
//! is about, by its id on the page, and the endpoint's position, never what
//! it holds.

use ferrofed_registry::console::{
    ConsoleEndpoint, ConsoleMember, ConsoleNode, ConsoleOrganisation,
};
use ferrofed_registry::id::{EndpointId, NodeId, OrganisationId, SystemId};
use ferrofed_registry::snapshot::{ConnectionType, EndpointStatus};

use crate::configure::model::{ConfigForm, FhirEndpointInput, QueryEndpointInput};
use crate::configure::plan::{Refused, at, given};
use crate::locale::{Msg, tr, tr_with};

/// How the form that gave a member was drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shown {
    /// Whether it is the form that adds a member, which refuses a `node_id`
    /// the gateway already holds.
    pub adding: bool,
    /// Whether the form showed the FHIR endpoints, by its `fhir_shown`
    /// marker; a form without it keeps the held FHIR endpoints whole.
    pub fhir: bool,
}

/// The id on the page of the field `name` of the member form.
#[must_use]
pub fn field(name: &str) -> String {
    format!("cfg-{name}")
}

/// The id on the page of the field `part` of the Query API endpoint row
/// `key`.
#[must_use]
pub fn query_field(key: &str, part: &str) -> String {
    format!("cfg-q-{key}-{part}")
}

/// The id on the page of the field `part` of the FHIR endpoint row `key`.
#[must_use]
pub fn fhir_field(key: &str, part: &str) -> String {
    format!("cfg-f-{key}-{part}")
}

/// The id of the button that adds a Query API endpoint row.
pub const ADD_QUERY: &str = "cfg-add-query";

/// The id of the button that adds a FHIR endpoint row.
pub const ADD_FHIR: &str = "cfg-add-fhir";

/// The form of a row key: 1 to 16 lowercase letters, digits and hyphens,
/// as the page and FerroUI's `repeat` module (`new-1`) write them, so a key names
/// a field id and nothing else.
fn row_key(key: &str) -> Result<&str, Refused> {
    let fits = !key.is_empty()
        && key.len() <= 16
        && key
            .chars()
            .all(|letter| letter.is_ascii_lowercase() || letter.is_ascii_digit() || letter == '-');
    if fits {
        Ok(key)
    } else {
        Err(Refused::from(crate::configure::plan::invalid(&tr(
            Msg::FormSEndpointRowsDoNot,
        ))))
    }
}

/// The member the form describes, and how its form was drawn.
///
/// # Errors
/// Returns a [`Refused`] naming its field for an id, a `system_id` or an
/// endpoint that does not read, and for a member with no Query API
/// endpoint.
pub fn member(form: &ConfigForm) -> Result<(ConsoleMember, Shown), Refused> {
    let id_form = tr(Msg::N164LettersDigitsStartingLetter);
    let organisation = OrganisationId::new(form.organisation.trim()).map_err(|_bad| {
        at(
            field("organisation"),
            tr_with(
                Msg::OrganisationIdNot,
                &[("id_form", (id_form).clone().into())],
            ),
        )
    })?;
    let node = NodeId::new(form.node.trim()).map_err(|_bad| {
        at(
            field("node"),
            tr_with(Msg::NodeIdNot, &[("id_form", (id_form).clone().into())]),
        )
    })?;
    let system_id = SystemId::new(form.system_id.trim())
        .map_err(|_bad| at(field("system_id"), tr(Msg::SystemIdNotOpenehrUid)))?;
    let mut endpoints = Vec::new();
    for (row, position) in form.query_endpoints.iter().zip(1_usize..) {
        endpoints.push(query_endpoint(row, position)?);
    }
    if endpoints.is_empty() {
        return Err(at(ADD_QUERY, tr(Msg::GiveLeastOneQueryApiEndpoint)));
    }
    for (row, position) in form.fhir_endpoints.iter().zip(1_usize..) {
        endpoints.push(fhir_endpoint(row, position)?);
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

/// The `endpoint_id` `id` of the `kind` endpoint at `position`, the field
/// `named` on the page.
fn endpoint_id(
    id: &str,
    (kind, position): (&str, usize),
    named: String,
) -> Result<EndpointId, Refused> {
    EndpointId::new(id.trim()).map_err(|_bad| {
        at(
            named,
            tr_with(
                Msg::EndpointIdEndpointNot164,
                &[
                    ("kind", (kind).to_string().into()),
                    ("position", (position).to_string().into()),
                ],
            ),
        )
    })
}

/// The base URL `url` of the `kind` endpoint at `position`, the field
/// `named` on the page: an absolute `http` or `https` URL with a host.
fn base_url(url: &str, (kind, position): (&str, usize), named: String) -> Result<String, Refused> {
    let url = url.trim();
    match url::Url::parse(url) {
        Ok(parsed)
            if matches!(parsed.scheme(), "http" | "https") && parsed.host_str().is_some() =>
        {
            Ok(url.to_owned())
        }
        _ => Err(at(
            named,
            tr_with(
                Msg::BaseUrlEndpointNotAbsoluteHttp,
                &[
                    ("kind", (kind).to_string().into()),
                    ("position", (position).to_string().into()),
                ],
            ),
        )),
    }
}

/// Whether an endpoint is in service, as the switch says.
const fn status(in_service: bool) -> EndpointStatus {
    if in_service {
        EndpointStatus::Active
    } else {
        EndpointStatus::Suspended
    }
}

/// One Query API endpoint, at `position`.
fn query_endpoint(row: &QueryEndpointInput, position: usize) -> Result<ConsoleEndpoint, Refused> {
    let key = row_key(&row.key)?;
    let word = tr(Msg::QueryApi);
    let kind = (word.as_str(), position);
    Ok(ConsoleEndpoint {
        id: endpoint_id(&row.id, kind, query_field(key, "id"))?,
        url: base_url(&row.url, kind, query_field(key, "url"))?,
        connection_type: ConnectionType::OpenehrRestQuery,
        status: status(row.in_service),
        consent_refusal_codes: Vec::new(),
        categories: Vec::new(),
        profiles: Vec::new(),
        ehr_id_system: None,
    })
}

/// One FHIR endpoint, at `position`: its `endpoint_id` and base URL, the
/// identifier system it is told the `ehr_id` in, at least one category and
/// at least one profile.
fn fhir_endpoint(row: &FhirEndpointInput, position: usize) -> Result<ConsoleEndpoint, Refused> {
    let key = row_key(&row.key)?;
    let word = tr(Msg::Fhir);
    let kind = (word.as_str(), position);
    let id = endpoint_id(&row.id, kind, fhir_field(key, "id"))?;
    let url = base_url(&row.url, kind, fhir_field(key, "url"))?;
    let system = given(&row.system).ok_or_else(|| {
        at(
            fhir_field(key, "system"),
            tr_with(
                Msg::FhirEndpointNamesNoIdentifierSystem,
                &[("position", (position).to_string().into())],
            ),
        )
    })?;
    let categories: Vec<String> = row
        .categories
        .iter()
        .filter_map(|category| given(category).map(str::to_owned))
        .collect();
    if categories.is_empty() {
        return Err(at(
            fhir_field(key, "categories"),
            tr_with(
                Msg::FhirEndpointServesNoCategoryChoose,
                &[("position", (position).to_string().into())],
            ),
        ));
    }
    let profiles: Vec<String> = row
        .profiles
        .iter()
        .filter_map(|profile| given(profile).map(str::to_owned))
        .collect();
    if profiles.is_empty() {
        return Err(at(
            fhir_field(key, "add-profile"),
            tr_with(
                Msg::FhirEndpointNamesNoProfileAdd,
                &[("position", (position).to_string().into())],
            ),
        ));
    }
    Ok(ConsoleEndpoint {
        id,
        url,
        connection_type: ConnectionType::Hl7FhirRest,
        status: status(row.in_service),
        consent_refusal_codes: Vec::new(),
        categories,
        profiles,
        ehr_id_system: Some(system.to_owned()),
    })
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
    use crate::read::Refusal;

    /// The FHIR endpoint `row` names at position 2, or the refusal's field
    /// and reason.
    fn read(row: &FhirEndpointInput) -> Result<ConsoleEndpoint, (Option<String>, String)> {
        fhir_endpoint(row, 2).map_err(|refused| match refused.refusal {
            Refusal::Invalid { reason } => (refused.field, reason),
            other => (refused.field, format!("{other:?}")),
        })
    }

    /// A change of a FHIR endpoint row.
    type Change = fn(&mut FhirEndpointInput);

    /// A FHIR endpoint row with every field filled.
    fn whole() -> FhirEndpointInput {
        FhirEndpointInput {
            key: String::from("0"),
            id: String::from("node-c-fhir"),
            url: String::from("https://cdr-c.example.org/fhir"),
            system: String::from("urn:oid:2.999.1"),
            categories: vec![String::from("Patient-Summaries")],
            profiles: vec![
                String::from("http://example.org/ps"),
                String::from("http://example.org/ps2"),
            ],
            in_service: false,
        }
    }

    #[test]
    #[expect(clippy::panic_in_result_fn, reason = "test assertions")]
    fn a_fhir_endpoint_reads_each_field_of_its_row() -> Result<(), (Option<String>, String)> {
        let endpoint = read(&whole())?;
        assert_eq!("node-c-fhir", endpoint.id.as_str());
        assert_eq!(ConnectionType::Hl7FhirRest, endpoint.connection_type);
        assert_eq!(EndpointStatus::Suspended, endpoint.status);
        assert_eq!(vec![String::from("Patient-Summaries")], endpoint.categories);
        assert_eq!(2, endpoint.profiles.len());
        assert_eq!(Some("urn:oid:2.999.1"), endpoint.ehr_id_system.as_deref());
        Ok(())
    }

    #[test]
    fn a_fhir_endpoint_missing_a_field_is_refused_at_that_field_by_its_position() {
        let cases: [(Change, &str); 6] = [
            (|row| row.system = String::from(" "), "cfg-f-0-system"),
            (|row| row.categories.clear(), "cfg-f-0-categories"),
            (
                |row| row.profiles = vec![String::new()],
                "cfg-f-0-add-profile",
            ),
            (
                |row| row.url = String::from("x.example.org/fhir"),
                "cfg-f-0-url",
            ),
            (|row| row.id = String::from("node c"), "cfg-f-0-id"),
            (|row| row.id = String::new(), "cfg-f-0-id"),
        ];
        for (change, field) in cases {
            let mut row = whole();
            change(&mut row);
            let refused = read(&row).err();
            assert!(
                refused.as_ref().is_some_and(|(at, reason)| {
                    at.as_deref() == Some(field) && reason.contains("FHIR endpoint 2")
                }),
                "{field}: {refused:?}"
            );
        }
    }

    #[test]
    fn a_url_holding_a_semicolon_or_a_percent_reads_as_it_is_typed() {
        for url in [
            "https://cdr-c.example.org/openehr;v=1/x;y",
            "https://cdr-c.example.org/a%3Bb%3bc%253Bd%2525%3B%20e",
            "https://cdr-c.example.org/%25%2%3%",
        ] {
            let row = QueryEndpointInput {
                key: String::from("n1"),
                id: String::from("node-c-pub"),
                url: url.to_owned(),
                in_service: true,
            };
            let read = query_endpoint(&row, 1).map(|endpoint| endpoint.url);
            assert_eq!(Ok(url.to_owned()), read.map_err(|refused| refused.field));
        }
    }

    #[test]
    fn a_query_endpoint_whose_url_is_not_absolute_http_is_refused_at_its_url() {
        for url in [
            "cdr.example.org/openehr",
            "ftp://cdr.example.org/openehr",
            "https://",
            "mailto:a@example.org",
        ] {
            let row = QueryEndpointInput {
                key: String::from("2"),
                id: String::from("node-c-pub"),
                url: url.to_owned(),
                in_service: true,
            };
            let refused = query_endpoint(&row, 3).err();
            assert_eq!(
                Some((
                    Some(String::from("cfg-q-2-url")),
                    Refusal::Invalid {
                        reason: String::from(
                            "The base URL of Query API endpoint 3 is not an absolute http or https URL."
                        )
                    }
                )),
                refused.map(|refused| (refused.field, refused.refusal)),
                "{url}"
            );
        }
    }

    #[test]
    fn a_row_key_that_is_not_the_pages_is_refused() {
        for key in ["", "A", "0\" onfocus=\"x", "abcdefghijklmnopq"] {
            assert!(row_key(key).is_err(), "{key}");
        }
        assert!(row_key("n12").is_ok());
        assert!(
            row_key("new-1").is_ok(),
            "a row FerroUI's repeat module adds"
        );
    }
}
