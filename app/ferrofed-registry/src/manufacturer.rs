// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The manufacturer of FerroFED, named in the running system.
//!
//! Regulation (EU) 2025/327 Art 30(1)(g) has a manufacturer of an EHR system
//! indicate "the name, registered trade name or registered trade mark, the
//! postal address, and the website, email address or other digital contact
//! details through which they can be contacted, in the EHR system", with "a
//! single point at which the manufacturer can be contacted". The manufacturer
//! is the Licensor `LICENSE` names.
//!
//! This file is the one place those details are written. The gateway's
//! `GET {base}/` and `OPTIONS {base}/`, its startup banner and `--version`
//! read [`MANUFACTURER`] from this module, and the operator console compiles
//! this same file for its footer, on the server and in the browser, since
//! the browser half links no FerroFED crate. The container image labels
//! repeat the values, and a test holds them to this file.

/// The manufacturer of an EHR system, as Art 30(1)(g) asks it to be named.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Manufacturer {
    /// The registered name.
    pub name: &'static str,
    /// The postal address, on one line.
    pub postal_address: &'static str,
    /// The email address, the single point of contact.
    pub email: &'static str,
    /// The website a person contacts the manufacturer through.
    pub website: &'static str,
}

/// The manufacturer of FerroFED: Cadasto B.V., the Licensor.
pub const MANUFACTURER: Manufacturer = Manufacturer {
    name: "Cadasto B.V.",
    postal_address: "Comeniusstraat 2d, 1817 MS Alkmaar, The Netherlands",
    email: "info@cadasto.com",
    website: "https://www.cadasto.com/contact/",
};

impl Manufacturer {
    /// Returns the manufacturer on one line: the name, the postal address
    /// and the single point of contact.
    #[must_use]
    pub fn line(&self) -> String {
        format!("{}, {}, {}", self.name, self.postal_address, self.email)
    }

    /// Returns what a FerroFED binary's `--version` prints after its name:
    /// `version`, then the manufacturer on one line and its website.
    #[must_use]
    pub fn version_text(&self, version: &str) -> String {
        format!(
            "{version}\nManufactured by {}\n{}",
            self.line(),
            self.website
        )
    }
}
