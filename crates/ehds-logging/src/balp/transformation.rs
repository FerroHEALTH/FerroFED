// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A semantic transformation written as one entity of the access record's
//! `AuditEvent`.
//!
//! The transformation rides in the same `AuditEvent` as the access that
//! applied it, so it is stored, kept and found with it, under the same
//! request id. BALP defines no element for it, so it is an entity of type
//! `4` named [`TRANSFORMATION_ENTITY`], whose `description` says what it
//! records and whose `detail` entries are named as [`detail`] lists them (no
//! specification governs the names: our own design).

use ihe_iti::balp::{Described, Detail, Entity, OTHER};

use super::detail;
use crate::transformation::{Elements, Transformation};

/// The name of the entity that records a semantic transformation.
pub const TRANSFORMATION_ENTITY: &str = "ehds-transformation";

/// The `description` of the entity that records a semantic transformation.
pub const TRANSFORMATION_DESCRIPTION: &str = "Semantic transformation: the mapping this access \
     applied between openEHR and the exchange format, with the elements it transcoded, left \
     untranscoded and dropped, counted by path and never by value (eHealth Network General \
     Guidelines Release 3.1 Art 6(3)).";

/// The entity `transformation` is written as.
pub(super) fn entity(transformation: &Transformation) -> Entity {
    let mapping = &transformation.mapping;
    let mut details = vec![Detail::new(
        detail::TRANSFORMATION_DIRECTION,
        transformation.direction.code(),
    )];
    details.extend(
        transformation
            .category
            .iter()
            .map(|category| Detail::new(detail::TRANSFORMATION_CATEGORY, category.token())),
    );
    details.extend(
        mapping
            .context
            .iter()
            .map(|context| Detail::new(detail::MAPPING_CONTEXT, context.clone())),
    );
    details.push(Detail::new(
        detail::MAPPING_TEMPLATE,
        mapping.template.clone(),
    ));
    details.extend(
        mapping
            .profile
            .iter()
            .map(|profile| Detail::new(detail::MAPPING_PROFILE, profile.clone())),
    );
    details.extend(
        mapping
            .files
            .iter()
            .map(|file| Detail::new(detail::MAPPING_FILE, file.clone())),
    );
    details.extend(
        mapping
            .package
            .iter()
            .map(|package| Detail::new(detail::MAPPING_PACKAGE, package.clone())),
    );
    details.extend(
        transformation
            .terminology
            .iter()
            .map(|version| Detail::new(detail::TERMINOLOGY_VERSION, version.clone())),
    );
    for (count, path, elements) in [
        (
            detail::TRANSCODED,
            detail::TRANSCODED_PATH,
            &transformation.transcoded,
        ),
        (
            detail::UNTRANSCODED,
            detail::UNTRANSCODED_PATH,
            &transformation.untranscoded,
        ),
        (
            detail::DROPPED,
            detail::DROPPED_PATH,
            &transformation.dropped,
        ),
    ] {
        details.extend(counted((count, path), elements));
    }
    Entity::Described(Described {
        what: None,
        kind: OTHER,
        role: None,
        name: Some(TRANSFORMATION_ENTITY.to_owned()),
        description: Some(TRANSFORMATION_DESCRIPTION.to_owned()),
        details,
        security_labels: Vec::new(),
    })
}

/// The `detail` entries of `elements`: its count under `count`, then each
/// path as `<path>=<count>` under `path`.
fn counted((count, path): (&str, &str), elements: &Elements) -> Vec<Detail> {
    let mut details = vec![Detail::new(count, elements.count().to_string())];
    details.extend(
        elements
            .paths()
            .iter()
            .map(|(at, held)| Detail::new(path, format!("{at}={held}"))),
    );
    details
}
