// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one JSON reading of a received document that admits no second
//! reading.
//!
//! RFC 8259 §4 says the names within an object "SHOULD be unique" and that
//! a parser meeting a repeated name may keep either value, so two readers of
//! one text can disagree about it
//! (<https://www.rfc-editor.org/rfc/rfc8259#section-4>). A received document
//! is read by the R4 model once, and the check, the subject rules and the
//! mapping all read that one value; a text with a repeated name is refused
//! before it is read at all, so no reader can see another document in it.

use std::collections::BTreeSet;
use std::fmt;

use serde::Deserialize;
use serde::Deserializer;
use serde::de::Error;
use serde::de::MapAccess;
use serde::de::SeqAccess;
use serde::de::Visitor;

/// Walks `text` as JSON and fails on the first object that repeats a name.
///
/// # Errors
///
/// Returns the `serde_json` error: a syntax error for text that is not JSON,
/// and a data error ([`serde_json::Error::is_data`]) for a repeated name.
pub(super) fn unique_names(text: &str) -> Result<(), serde_json::Error> {
    serde_json::from_str::<Unique>(text).map(|_| ())
}

/// Any JSON value whose objects repeat no name.
struct Unique;

impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(UniqueVisitor)
    }
}

/// The visitor behind [`Unique`].
struct UniqueVisitor;

impl<'de> Visitor<'de> for UniqueVisitor {
    type Value = Unique;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value whose objects repeat no name")
    }

    fn visit_bool<E: Error>(self, _: bool) -> Result<Unique, E> {
        Ok(Unique)
    }

    fn visit_i64<E: Error>(self, _: i64) -> Result<Unique, E> {
        Ok(Unique)
    }

    fn visit_u64<E: Error>(self, _: u64) -> Result<Unique, E> {
        Ok(Unique)
    }

    fn visit_f64<E: Error>(self, _: f64) -> Result<Unique, E> {
        Ok(Unique)
    }

    fn visit_str<E: Error>(self, _: &str) -> Result<Unique, E> {
        Ok(Unique)
    }

    fn visit_unit<E: Error>(self) -> Result<Unique, E> {
        Ok(Unique)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut items: A) -> Result<Unique, A::Error> {
        while items.next_element::<Unique>()?.is_some() {}
        Ok(Unique)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut members: A) -> Result<Unique, A::Error> {
        let mut seen = BTreeSet::new();
        while let Some(name) = members.next_key::<String>()? {
            members.next_value::<Unique>()?;
            if !seen.insert(name) {
                return Err(A::Error::custom("an object repeats a name"));
            }
        }
        Ok(Unique)
    }
}
