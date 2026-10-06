// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The manufacturer FerroFED names in the running system, pinned as
//! Regulation (EU) 2025/327 Art 30(1)(g) asks for it: the name, the postal
//! address, and the digital contact with its single point.

use std::error::Error;

use ferrofed_registry::manufacturer::MANUFACTURER;

#[test]
fn the_manufacturer_is_the_licensor_with_its_address_and_contact() {
    assert_eq!("Cadasto B.V.", MANUFACTURER.name);
    assert_eq!(
        "Comeniusstraat 2d, 1817 MS Alkmaar, The Netherlands",
        MANUFACTURER.postal_address
    );
    assert_eq!("info@cadasto.com", MANUFACTURER.email);
    assert_eq!("https://www.cadasto.com/contact/", MANUFACTURER.website);
}

#[test]
fn the_one_line_form_names_the_address_and_the_single_point_of_contact() {
    assert_eq!(
        "Cadasto B.V., Comeniusstraat 2d, 1817 MS Alkmaar, The Netherlands, info@cadasto.com",
        MANUFACTURER.line()
    );
}

#[test]
fn the_version_text_follows_the_version_with_the_manufacturer() {
    assert_eq!(
        "9.9.9\nManufactured by Cadasto B.V., Comeniusstraat 2d, 1817 MS Alkmaar, The Netherlands, info@cadasto.com\nhttps://www.cadasto.com/contact/",
        MANUFACTURER.version_text("9.9.9")
    );
}

#[test]
fn the_manufacturer_is_the_licensor_the_licence_names() -> Result<(), Box<dyn Error>> {
    let licence = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../LICENSE"))?;
    let licensor = licence
        .lines()
        .find_map(|line| line.strip_prefix("Licensor:"))
        .map(str::trim);
    assert_eq!(Some(MANUFACTURER.name), licensor);
    Ok(())
}

#[test]
fn the_manufacturer_is_written_with_its_field_names() -> Result<(), Box<dyn Error>> {
    let written = serde_json::to_string(&MANUFACTURER)?;
    assert_eq!(
        r#"{"name":"Cadasto B.V.","postal_address":"Comeniusstraat 2d, 1817 MS Alkmaar, The Netherlands","email":"info@cadasto.com","website":"https://www.cadasto.com/contact/"}"#,
        written
    );
    Ok(())
}
