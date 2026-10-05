// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The path parameters of a routed request, each parsed as the openEHR
//! identifier it names before the path travels (§5.4.1, N33).
//!
//! The path reaches the node as the client sent it, byte for byte, once
//! every identifier in it parses: `openehr-base` keeps an `OBJECT_VERSION_ID`
//! verbatim, so its print is the client's text, and an openEHR uid is never
//! rewritten (N22).

use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_base::v1_3::base_types::identification::lexical::is_uuid;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use openehr_its::rest::routes::{IdentifierClass, Param, ParamKind, ParamLocation, RouteMatch};

use super::kind::{fits, is_free_text};
use super::{Carrier, Expected, MalformedValue};

/// The path parameter the routing reads as the EHR's `HIER_OBJECT_ID`.
const EHR_ID: &str = "ehr_id";

/// What a path parameter must parse as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PathValue {
    /// The `ehr_id`, which the routing parses itself.
    Routed,
    /// An identifier class or a kind `openehr-its`'s table states.
    Expected(Expected),
    /// Free text the gateway cannot classify.
    Free,
}

/// What the path parameter `param` must parse as.
///
/// A schema `uuid` format is an openEHR `UUID`, the narrowest form. Any
/// other path parameter that carries an openEHR identifier parses as the
/// identifier class `openehr-its`'s table states for it
/// ([`IdentifierClass`]), and one that carries none is held to its kind.
pub(super) fn expected(param: &Param) -> PathValue {
    if param.name == EHR_ID {
        return PathValue::Routed;
    }
    match (param.kind, param.identifier) {
        (ParamKind::Uuid, _) => PathValue::Expected(Expected::Kind(ParamKind::Uuid)),
        (_, Some(IdentifierClass::ObjectVersion)) => PathValue::Expected(Expected::ObjectVersionId),
        (_, Some(IdentifierClass::UidBased)) => PathValue::Expected(Expected::UidBasedId),
        (_, Some(IdentifierClass::HierObject)) => PathValue::Expected(Expected::HierObjectId),
        (kind, None) if is_free_text(&kind) => PathValue::Free,
        (kind, None) => PathValue::Expected(Expected::Kind(kind)),
    }
}

/// Holds each path parameter of `operation` to the identifier it names.
///
/// The `ehr_id` is left to the routing, which reads it as a
/// `HIER_OBJECT_ID` and answers a malformed one itself (§12.5).
pub(super) fn held(operation: &RouteMatch) -> Result<(), MalformedValue> {
    for (index, segment) in operation.path_params.iter().enumerate() {
        let declared = operation
            .params
            .iter()
            .find(|param| param.location == ParamLocation::Path && param.name == segment.name);
        let expected = match declared.map(expected) {
            Some(PathValue::Expected(expected)) => expected,
            Some(PathValue::Routed | PathValue::Free) => continue,
            None => Expected::Kind(ParamKind::Unspecified),
        };
        let parsed = segment
            .decoded()
            .is_ok_and(|value| parses(expected, &value));
        if !parsed {
            return Err(MalformedValue {
                carrier: Carrier::Path {
                    position: index.saturating_add(1),
                    name: segment.name,
                },
                expected,
            });
        }
    }
    Ok(())
}

/// Whether `value` parses as `expected`.
fn parses(expected: Expected, value: &str) -> bool {
    match expected {
        Expected::Kind(ParamKind::Uuid) => is_uuid(value),
        Expected::Kind(kind) => fits(&kind, value, false),
        Expected::ObjectVersionId => ObjectVersionId::new(value).is_ok(),
        Expected::HierObjectId => HierObjectId::new(value).is_ok(),
        Expected::UidBasedId => {
            ObjectVersionId::new(value).is_ok() || HierObjectId::new(value).is_ok()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::held;
    use crate::declared::{Carrier, Expected};
    use http::Method;
    use openehr_its::rest::routes::{Lookup, ParamKind, RouteMatch, lookup};

    const EHR: &str = "/ehr/7d44b88c-4199-4bad-97dc-d78268e01398";
    const VERSION: &str = "8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::1";
    const OBJECT: &str = "8849182c-82ad-4088-a07f-48ead4180515";

    fn operation(method: &Method, path: &str) -> RouteMatch {
        match lookup(method, path) {
            Lookup::Matched(matched) => matched,
            other => panic!("{method} {path} names no operation: {other:?}"),
        }
    }

    fn carrier(method: &Method, path: &str) -> Option<Carrier> {
        held(&operation(method, path))
            .err()
            .map(|refused| refused.carrier())
    }

    // conformance: CP-26
    #[test]
    fn a_version_uid_parses_as_an_object_version_id() {
        let read = |uid: &str| format!("{EHR}/ehr_status/{uid}");
        assert_eq!(None, carrier(&Method::GET, &read(VERSION)));
        assert_eq!(
            Some(Carrier::Path {
                position: 2,
                name: "version_uid"
            }),
            carrier(&Method::GET, &read("ffd-test-0010"))
        );
    }

    // conformance: CP-26
    #[test]
    fn a_uid_based_id_parses_as_a_version_or_an_object_id() {
        let read = |uid: &str| format!("{EHR}/composition/{uid}");
        assert_eq!(None, carrier(&Method::GET, &read(VERSION)));
        assert_eq!(None, carrier(&Method::GET, &read(OBJECT)));
        assert_eq!(
            Some(Carrier::Path {
                position: 2,
                name: "uid_based_id"
            }),
            carrier(&Method::GET, &read("O'Sentinel%20one"))
        );
    }

    // conformance: CP-26
    #[test]
    fn the_uid_based_id_of_a_delete_parses_as_an_object_version_id() {
        let delete = |uid: &str| format!("/demographic/person/{uid}");
        assert_eq!(None, carrier(&Method::DELETE, &delete(VERSION)));
        let refusal = held(&operation(&Method::DELETE, &delete(OBJECT))).expect_err("a refusal");
        assert_eq!(Expected::ObjectVersionId, refusal.expected());
        assert_eq!(
            None,
            carrier(&Method::GET, &delete(OBJECT)),
            "a read takes either form"
        );
    }

    // conformance: CP-26
    #[test]
    fn a_uuid_path_parameter_is_a_canonical_uuid() {
        let read = |uid: &str| format!("{EHR}/versioned_composition/{uid}");
        assert_eq!(None, carrier(&Method::GET, &read(OBJECT)));
        for refused in ["8849182c82ad4088a07f48ead4180515", "ffd-test-0010", VERSION] {
            let refusal = held(&operation(&Method::GET, &read(refused))).expect_err("a refusal");
            assert_eq!(
                Expected::Kind(ParamKind::Uuid),
                refusal.expected(),
                "{refused}"
            );
            assert!(!refusal.to_string().contains("ffd-test"), "{refusal}");
        }
    }

    #[test]
    fn the_ehr_id_is_left_to_the_routing_and_a_tag_key_is_free_text() {
        assert_eq!(None, carrier(&Method::GET, "/ehr/ffd-test-0010"));
        assert_eq!(
            None,
            carrier(
                &Method::DELETE,
                &format!("{EHR}/composition/{VERSION}/tags/ffd-test-0010")
            )
        );
    }
}
