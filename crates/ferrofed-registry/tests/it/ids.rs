// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Each identifier holds its own form: the registry-owned ids their token
//! rule, a `system_id` an openEHR `uid`, an `ehr_id` a `HIER_OBJECT_ID`.

use std::error::Error;

use ferrofed_registry::error::{IdError, IdKind};
use ferrofed_registry::id::{EhrId, EndpointId, MAX_ID_LEN, NodeId, OrganisationId, SystemId};

#[test]
fn a_registry_id_takes_letters_digits_dots_dashes_and_underscores() -> Result<(), Box<dyn Error>> {
    for value in ["node-1", "Node_1.a", "7", "a".repeat(MAX_ID_LEN).as_str()] {
        assert_eq!(
            NodeId::new(value)?.as_str(),
            value,
            "{value:?} is a node_id"
        );
        assert_eq!(
            EndpointId::new(value)?.to_string(),
            value,
            "{value:?} is an endpoint_id"
        );
        assert_eq!(
            OrganisationId::new(value)?.as_str(),
            value,
            "{value:?} is an organisation id"
        );
    }
    Ok(())
}

#[test]
fn a_registry_id_outside_its_form_is_refused() {
    assert_eq!(
        NodeId::new(""),
        Err(IdError::Empty { kind: IdKind::Node }),
        "empty"
    );
    assert!(
        matches!(
            EndpointId::new("a".repeat(MAX_ID_LEN + 1)),
            Err(IdError::TooLong {
                kind: IdKind::Endpoint,
                ..
            })
        ),
        "one byte too long"
    );
    for value in [
        "-lead",
        ".lead",
        "_lead",
        "has space",
        "tab\tin",
        "colon:in",
        "é",
    ] {
        assert!(
            matches!(
                OrganisationId::new(value),
                Err(IdError::Malformed {
                    kind: IdKind::Organisation,
                    ..
                })
            ),
            "{value:?} is refused"
        );
    }
}

#[test]
fn a_system_id_is_an_openehr_uid() -> Result<(), Box<dyn Error>> {
    for value in [
        "cdr1.rso.example",
        "2.999.20.1",
        "6f2a51a4-1b8e-4f8b-9a4c-1f6c2b1d7e30",
    ] {
        assert_eq!(
            SystemId::new(value)?.as_str(),
            value,
            "{value:?} is a uid, stored as written"
        );
    }
    for value in ["", "not a uid", "-leading.example"] {
        assert!(
            matches!(SystemId::new(value), Err(IdError::SystemId { .. })),
            "{value:?} is refused"
        );
    }
    Ok(())
}

#[test]
fn an_ehr_id_is_a_hier_object_id_compared_without_case() -> Result<(), Box<dyn Error>> {
    let lower = EhrId::new("6f2a51a4-1b8e-4f8b-9a4c-1f6c2b1d7e30")?;
    let upper = EhrId::new("6F2A51A4-1B8E-4F8B-9A4C-1F6C2B1D7E30")?;
    assert_eq!(
        lower, upper,
        "master05: identical apart from case is the same"
    );
    assert_eq!(
        upper.as_str(),
        "6F2A51A4-1B8E-4F8B-9A4C-1F6C2B1D7E30",
        "stored as written"
    );
    assert!(
        matches!(EhrId::new("no uid here"), Err(IdError::EhrId { .. })),
        "refused"
    );
    Ok(())
}
