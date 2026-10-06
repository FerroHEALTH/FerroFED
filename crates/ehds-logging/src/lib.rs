// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The European logging software component of an EHR system (Regulation (EU)
//! 2025/327 Art 2(2)(o), Art 25(1), Annex II 3.2 to 3.4).
//!
//! The Regulation asks an EHR system that gives healthcare providers or
//! other individuals access to personal electronic health data to record, on
//! every access event or group of events, who accessed the data, which
//! natural person, the categories of the data, when, and where the data came
//! from (Annex II 3.2). This crate is that record and what classifies it:
//!
//! - [`record`]: the [`AccessRecord`](record::AccessRecord), with the data
//!   subject and the purpose of use beside the five points of 3.2;
//! - [`category`]: the Art 14(1) categories and the national categories a
//!   deployment declares;
//! - [`map`] and [`classify`]: the deployment's map from template and
//!   archetype ids to categories, and the classification of one access by
//!   the model ids of what it reached;
//! - [`retention`]: how long a record is kept, by its categories and its
//!   origins, never under the three years of Art 9(2);
//! - [`sink`]: where records go, and why one could not be stored;
//! - `balp` (feature `balp`): the record written as an IHE BALP `AuditEvent`
//!   through `ihe-iti`, and the sink over an `ihe-iti` audit recorder.
//!
//! The crate depends on no interoperability component and on nothing of the
//! system that builds the records, so the logging component stays independent
//! of the interoperability component (Art 2(2)(n), (o)).
//!
//! # Examples
//!
//! ```
//! use std::collections::BTreeMap;
//!
//! use ehds_logging::category::Category;
//! use ehds_logging::classify::{Basis, Evidence, RootObject};
//! use ehds_logging::map::{CategoryMap, Declared};
//!
//! let templates = BTreeMap::from([(
//!     "Example Lab Report.v1".to_owned(),
//!     Declared::Codes(vec!["medical-test-result".to_owned()]),
//! )]);
//! let map = CategoryMap::declare(&[], &templates, &BTreeMap::new())?;
//! let read = RootObject {
//!     template_id: Some("Example Lab Report.v1".to_owned()),
//!     ..RootObject::default()
//! };
//! let classified = map.classify(&Evidence::reached(Basis::Returned, vec![read]));
//! assert!(classified.categories().contains_key(&Category::MedicalTestResult));
//! assert!(classified.unclassified().is_none());
//! # Ok::<(), ehds_logging::map::MapError>(())
//! ```

#![doc(test(attr(deny(warnings))))]

#[cfg(feature = "balp")]
pub mod balp;
pub mod category;
pub mod classify;
pub mod map;
pub mod record;
pub mod retention;
pub mod sink;
