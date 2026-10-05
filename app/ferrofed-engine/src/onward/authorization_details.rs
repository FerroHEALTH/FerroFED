// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `authorization_details` of a token request (RFC 9396), taken from
//! configuration as JSON text and sent as written.
//!
//! The text is held to the shape RFC 9396 §2 gives the parameter before it
//! is ever sent: a JSON array of one or more objects, each with a non-empty
//! string `type`, and each common data field of §2.2 it uses of the type
//! that section gives it. No object of it may repeat a name. The members a
//! `type` defines beyond those are the authorization server's to judge
//! (RFC 9396 §2.1, §5), so they pass as written.
//!
//! The Annex B §B.4a track carries its healthcare attributes this way, in
//! an object of type `nl-gis-v1` (§B.4a.3).

use std::collections::BTreeSet;
use std::fmt;

use serde::Deserialize;
use serde_json::value::RawValue;

/// The `authorization_details` of one grant, as written, and the types it
/// names.
///
/// `Debug` shows the types alone.
#[derive(Clone)]
pub struct AuthorizationDetails {
    text: Box<RawValue>,
    types: BTreeSet<String>,
}

/// An `authorization_details` text that does not have the shape of RFC 9396
/// §2.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AuthorizationDetailsError {
    /// The text is no JSON, or an object of it repeats a name.
    #[error("the authorization_details is not JSON whose objects repeat no name")]
    Json(#[source] serde_json::Error),
    /// The JSON is not an array of objects, or a common data field is not
    /// of the type RFC 9396 §2.2 gives it.
    #[error(
        "the authorization_details is not an array of objects whose common fields have their RFC 9396 §2.2 types"
    )]
    Shape(#[source] serde_json::Error),
    /// The array holds no object.
    #[error("the authorization_details names no authorization details object (RFC 9396 §2)")]
    Empty,
    /// An object has no `type`, or an empty one (RFC 9396 §2).
    #[error("authorization details object {index} has no type (RFC 9396 §2)")]
    Untyped {
        /// The object's position in the array, from zero.
        index: usize,
    },
}

/// One authorization details object, read for its shape (RFC 9396 §2, §2.2):
/// its `type` and the common data fields, every other member left unread.
#[derive(Deserialize)]
pub(crate) struct Detail {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(rename = "locations")]
    _locations: Option<Vec<String>>,
    #[serde(rename = "actions")]
    _actions: Option<Vec<String>>,
    #[serde(rename = "datatypes")]
    _datatypes: Option<Vec<String>>,
    #[serde(rename = "identifier")]
    _identifier: Option<String>,
    #[serde(rename = "privileges")]
    _privileges: Option<Vec<String>>,
}

impl AuthorizationDetails {
    /// Reads `text` as the value of the `authorization_details` parameter
    /// (RFC 9396 §2).
    ///
    /// # Errors
    ///
    /// Returns [`AuthorizationDetailsError::Json`] for text that is no JSON
    /// or repeats a name in an object, [`AuthorizationDetailsError::Shape`]
    /// for JSON that is no array of objects or whose common data fields are
    /// of another type, [`AuthorizationDetailsError::Empty`] for an empty
    /// array, and [`AuthorizationDetailsError::Untyped`] for an object
    /// without a `type`.
    pub fn parse(text: &str) -> Result<Self, AuthorizationDetailsError> {
        oauth_server_metadata::repeats_no_name(text.as_bytes())
            .map_err(AuthorizationDetailsError::Json)?;
        let types = types_of(text)?;
        let text = serde_json::from_str::<Box<RawValue>>(text.trim())
            .map_err(AuthorizationDetailsError::Json)?;
        Ok(Self { text, types })
    }

    /// The text the token request carries, as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.text.get()
    }

    /// Every `type` the objects name, in order, each once.
    #[must_use]
    pub fn types(&self) -> &BTreeSet<String> {
        &self.types
    }
}

/// The types `text`, an `authorization_details` value, names, once it is
/// known to have the shape of RFC 9396 §2.
pub(crate) fn types_of(text: &str) -> Result<BTreeSet<String>, AuthorizationDetailsError> {
    let details =
        serde_json::from_str::<Vec<Detail>>(text).map_err(AuthorizationDetailsError::Shape)?;
    if details.is_empty() {
        return Err(AuthorizationDetailsError::Empty);
    }
    let mut types = BTreeSet::new();
    for (index, detail) in details.into_iter().enumerate() {
        match detail.kind {
            Some(kind) if !kind.is_empty() => {
                types.insert(kind);
            }
            _ => return Err(AuthorizationDetailsError::Untyped { index }),
        }
    }
    Ok(types)
}

impl fmt::Debug for AuthorizationDetails {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthorizationDetails")
            .field("types", &self.types)
            .finish_non_exhaustive()
    }
}

impl PartialEq for AuthorizationDetails {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for AuthorizationDetails {}

#[cfg(test)]
mod tests {
    use super::*;

    const NL_GIS: &str = r#"[{"type": "nl-gis-v1", "purpose_of_use": "http://terminology.hl7.org/CodeSystem/v3-ActReason|TREAT", "locations": ["https://fhir.example.org/fhir"], "locations_organization_id": "urn:oid:2.999.1"}]"#;

    #[test]
    fn the_memo_shape_is_taken_and_sent_as_written() {
        let details = AuthorizationDetails::parse(NL_GIS).expect("taken");
        assert_eq!(NL_GIS, details.as_str());
        assert_eq!(
            vec!["nl-gis-v1"],
            details
                .types()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_text_outside_rfc_9396_section_2_is_refused() {
        for (text, why) in [
            ("not json", "no JSON"),
            (r#"{"type": "a"}"#, "an object, not an array"),
            ("[]", "an empty array"),
            ("[1]", "an array of numbers"),
            (r#"[{"actions": ["read"]}]"#, "no type"),
            (r#"[{"type": ""}]"#, "an empty type"),
            (r#"[{"type": 7}]"#, "a type that is no string"),
            (
                r#"[{"type": "a", "locations": "https://x"}]"#,
                "locations not an array",
            ),
            (r#"[{"type": "a", "actions": [1]}]"#, "actions not strings"),
            (
                r#"[{"type": "a", "identifier": ["x"]}]"#,
                "identifier not a string",
            ),
            (r#"[{"type": "a", "type": "b"}]"#, "a repeated name"),
        ] {
            assert!(AuthorizationDetails::parse(text).is_err(), "{why}");
        }
    }

    #[test]
    fn debug_shows_the_types_and_no_member() {
        let details = AuthorizationDetails::parse(NL_GIS).expect("taken");
        let shown = format!("{details:?}");
        assert!(shown.contains("nl-gis-v1"), "{shown}");
        assert!(!shown.contains("TREAT"), "{shown}");
    }
}
