// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient reference never shows its value (§5.4.1, N33).

use std::error::Error;

use ferrofed_identity::role::patient::{IdentifierNamespace, PatientRef, PatientRefError};

use crate::support::PATIENT_VALUE;

#[test]
fn debug_and_display_never_show_the_value() -> Result<(), Box<dyn Error>> {
    let patient = PatientRef::new(IdentifierNamespace::new("2.999.1")?, PATIENT_VALUE.into())?;
    let rendered = format!("{patient} | {patient:?} | {patient:#?}");
    assert!(
        !rendered.contains(PATIENT_VALUE),
        "the value is redacted: {rendered}"
    );
    assert!(
        rendered.contains("2.999.1"),
        "the namespace is shown: {rendered}"
    );
    assert!(
        rendered.contains(r#"value: "***""#),
        "the family's placeholder: {rendered}"
    );
    assert_eq!(
        patient.namespace().as_str(),
        "2.999.1",
        "the namespace as written"
    );
    Ok(())
}

#[test]
fn an_empty_value_or_namespace_is_refused() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        IdentifierNamespace::new(""),
        Err(PatientRefError::EmptyNamespace),
        "an empty namespace"
    );
    assert!(
        matches!(
            PatientRef::new(IdentifierNamespace::new("2.999.1")?, "".into()),
            Err(PatientRefError::EmptyValue)
        ),
        "an empty value"
    );
    Ok(())
}
