// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The record of each semantic transformation the FHIR face applies, built
//! from the interoperability component's report for the logging component
//! (eHealth Network General Guidelines Release 3.1 Art 6(3)).
//!
//! The two components never name each other (Regulation (EU) 2025/327 Art
//! 2(2)(n), (o)), so the server, which links both, carries each fact across:
//! the report holds paths and counts alone, and so does the record.

use eehrxf::transformation::{Report, Tally};
use ehds_logging::category::Category;
use ehds_logging::transformation::{Direction, Elements, Mapping, Transformation};

/// The record of the transformation `report` describes.
pub(crate) fn of(report: &Report) -> Transformation {
    let pin = report.pin();
    Transformation {
        direction: match report.direction() {
            eehrxf::transformation::Direction::Provided => Direction::Provided,
            eehrxf::transformation::Direction::Received => Direction::Received,
        },
        category: Category::priority(report.category().code()),
        mapping: Mapping {
            context: pin.context.clone(),
            template: pin.template.clone(),
            profile: pin.profile.clone(),
            files: pin.files.clone(),
            package: pin.package.clone(),
        },
        terminology: report.terminology().iter().cloned().collect(),
        transcoded: elements(report.transcoded()),
        untranscoded: elements(report.untranscoded()),
        dropped: elements(report.dropped()),
    }
}

/// The elements `tally` counts, at the paths it names them by.
fn elements(tally: &Tally) -> Elements {
    Elements::new(tally.count(), tally.paths().clone())
}
