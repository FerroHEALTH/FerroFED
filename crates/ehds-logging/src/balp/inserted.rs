// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mark of a write whose information the data subject, or their
//! representative, inserted.
//!
//! Regulation (EU) 2025/327 Art 5 asks that information a natural person or
//! their representative of Art 4(2) inserts in the person's EHR "be clearly
//! distinguishable as having been inserted by the natural person or by his
//! or her representative". The written data carries that mark; the record
//! of the write says so in one entity of its own, `ehds-subject-inserted`
//! for the person and `ehds-representative-inserted` for a representative,
//! whose `description` states the mark in words and whose one `detail`
//! entry, named as the entity, is set to `true`, so a reader of the record
//! need not read the written data to know it.

use ihe_iti::balp::{Described, Detail, Entity, OTHER};

use crate::record::Insertion;

/// The name of the entity that marks a write the data subject inserted,
/// and of its one `detail` entry, whose value is `true`.
pub const ENTITY: &str = "ehds-subject-inserted";

/// The `description` of the entity that marks a write the data subject
/// inserted.
pub const DESCRIPTION: &str = "Inserted by the data subject: the information \
     this access wrote was inserted by the natural person under a grant of \
     their own, and the written data marks it as inserted by them \
     (Regulation (EU) 2025/327 Art 5).";

/// The name of the entity that marks a write a representative of the data
/// subject inserted, and of its one `detail` entry, whose value is `true`.
pub const REPRESENTATIVE_ENTITY: &str = "ehds-representative-inserted";

/// The `description` of the entity that marks a write a representative of
/// the data subject inserted.
pub const REPRESENTATIVE_DESCRIPTION: &str = "Inserted by a representative \
     of the data subject: the information this access wrote was inserted, \
     under the natural person's grant, by a natural person who acts for them \
     as a proxy or a legal representative, and the written data names that \
     representative as its provider, never the natural person (Regulation \
     (EU) 2025/327 Art 4(2), Art 5).";

/// The entity that marks a write `inserted` says who inserted.
pub(super) fn entity(inserted: Insertion) -> Entity {
    let (name, description) = match inserted {
        Insertion::Subject => (ENTITY, DESCRIPTION),
        Insertion::Representative => (REPRESENTATIVE_ENTITY, REPRESENTATIVE_DESCRIPTION),
    };
    Entity::Described(Described {
        what: None,
        kind: OTHER,
        role: None,
        name: Some(name.to_owned()),
        description: Some(description.to_owned()),
        details: vec![Detail::new(name, "true")],
        security_labels: Vec::new(),
    })
}
