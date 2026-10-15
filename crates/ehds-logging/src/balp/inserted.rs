// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mark of a write whose information the data subject inserted.
//!
//! Regulation (EU) 2025/327 Art 5 asks that information a natural person
//! inserts in their own EHR "be clearly distinguishable as having been
//! inserted by the natural person". The written data carries that mark; the
//! record of the write says so in one entity of its own,
//! `ehds-subject-inserted`, whose `description` states the mark in words and
//! whose one `detail` entry is `ehds-subject-inserted` set to `true`, so a
//! reader of the record need not read the written data to know it.

use ihe_iti::balp::{Described, Detail, Entity, OTHER};

/// The name of the entity that marks a write the data subject inserted,
/// and of its one `detail` entry, whose value is `true`.
pub const ENTITY: &str = "ehds-subject-inserted";

/// The `description` of the entity that marks a write the data subject
/// inserted.
pub const DESCRIPTION: &str = "Inserted by the data subject: the information \
     this access wrote was inserted by the natural person under a grant of \
     their own, and the written data marks it as inserted by them \
     (Regulation (EU) 2025/327 Art 5).";

/// The entity that marks a write the data subject inserted.
pub(super) fn entity() -> Entity {
    Entity::Described(Described {
        what: None,
        kind: OTHER,
        role: None,
        name: Some(ENTITY.to_owned()),
        description: Some(DESCRIPTION.to_owned()),
        details: vec![Detail::new(ENTITY, "true")],
        security_labels: Vec::new(),
    })
}
