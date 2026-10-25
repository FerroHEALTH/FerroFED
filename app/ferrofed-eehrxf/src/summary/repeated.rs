// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! How many entries of one archetype a composition holds (hazard I-03 of the
//! clinical safety risk file).
//!
//! A FHIRconnect context maps a composition to one resource, rooted at the
//! archetype of its start model, and FHIRconnect 1.0.0 binds `$archetype` to
//! "the root of the archetype" (`basics/Variables.adoc`). A composition that
//! holds that archetype twice, two medications in one medication list, would
//! map to one resource that mixes the two, one of them lost. The summary
//! counts the start archetype's occurrences first and leaves a composition
//! that holds it more than once out of the section, naming it, rather than
//! write a resource that mixes its entries. The count and the rule are our
//! own design: no specification governs them.

use std::fmt;

use serde::Deserializer;
use serde::de::{self, DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};

/// The RM attribute that names a node's archetype or archetype node id.
const NODE_ID: &str = "archetype_node_id";

/// Returns how many objects of the canonical-JSON composition `text` carry
/// the `archetype_node_id` `archetype`, at any depth.
///
/// # Errors
///
/// The JSON reader's error for a text that is no JSON.
pub fn occurrences(text: &str, archetype: &str) -> Result<usize, serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let count = Counting { archetype }.deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(count)
}

/// Counts the objects under one JSON value that name `archetype`.
#[derive(Clone, Copy)]
struct Counting<'a> {
    archetype: &'a str,
}

impl<'de> DeserializeSeed<'de> for Counting<'_> {
    type Value = usize;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<usize, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Counting<'_> {
    type Value = usize;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_bool<E: de::Error>(self, _value: bool) -> Result<usize, E> {
        Ok(0)
    }

    fn visit_i64<E: de::Error>(self, _value: i64) -> Result<usize, E> {
        Ok(0)
    }

    fn visit_u64<E: de::Error>(self, _value: u64) -> Result<usize, E> {
        Ok(0)
    }

    fn visit_f64<E: de::Error>(self, _value: f64) -> Result<usize, E> {
        Ok(0)
    }

    fn visit_str<E: de::Error>(self, _value: &str) -> Result<usize, E> {
        Ok(0)
    }

    fn visit_unit<E: de::Error>(self) -> Result<usize, E> {
        Ok(0)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<usize, A::Error> {
        let mut count: usize = 0;
        while let Some(found) = seq.next_element_seed(self)? {
            count = count.saturating_add(found);
        }
        Ok(count)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<usize, A::Error> {
        let mut count: usize = 0;
        while let Some(key) = map.next_key::<String>()? {
            if key == NODE_ID {
                let named = map.next_value::<NodeId>()?;
                if named.0.as_deref() == Some(self.archetype) {
                    count = count.saturating_add(1);
                }
            } else {
                count = count.saturating_add(map.next_value_seed(self)?);
            }
        }
        Ok(count)
    }
}

/// An `archetype_node_id` value: its text when it is a string.
struct NodeId(Option<String>);

impl<'de> de::Deserialize<'de> for NodeId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(NodeIdVisitor)
    }
}

/// Reads an `archetype_node_id` value.
struct NodeIdVisitor;

impl<'de> Visitor<'de> for NodeIdVisitor {
    type Value = NodeId;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an archetype node id")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<NodeId, E> {
        Ok(NodeId(Some(value.to_owned())))
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<NodeId, A::Error> {
        IgnoredAny.visit_map(map).map(|_ignored| NodeId(None))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<NodeId, A::Error> {
        IgnoredAny.visit_seq(seq).map(|_ignored| NodeId(None))
    }

    fn visit_bool<E: de::Error>(self, _value: bool) -> Result<NodeId, E> {
        Ok(NodeId(None))
    }

    fn visit_i64<E: de::Error>(self, _value: i64) -> Result<NodeId, E> {
        Ok(NodeId(None))
    }

    fn visit_u64<E: de::Error>(self, _value: u64) -> Result<NodeId, E> {
        Ok(NodeId(None))
    }

    fn visit_f64<E: de::Error>(self, _value: f64) -> Result<NodeId, E> {
        Ok(NodeId(None))
    }

    fn visit_unit<E: de::Error>(self) -> Result<NodeId, E> {
        Ok(NodeId(None))
    }
}

#[cfg(test)]
mod tests {
    use super::occurrences;

    #[test]
    fn every_object_naming_the_archetype_is_counted_at_any_depth() {
        let text = r#"{"archetype_node_id":"openEHR-EHR-COMPOSITION.c.v1","content":[
            {"archetype_node_id":"openEHR-EHR-EVALUATION.e.v1","data":{"archetype_node_id":"at0001"}},
            {"archetype_node_id":"openEHR-EHR-SECTION.s.v1","items":[{"archetype_node_id":"openEHR-EHR-EVALUATION.e.v1"}]}]}"#;
        assert_eq!(occurrences(text, "openEHR-EHR-EVALUATION.e.v1").unwrap(), 2);
        assert_eq!(occurrences(text, "openEHR-EHR-ACTION.a.v1").unwrap(), 0);
    }

    #[test]
    fn a_text_that_is_no_json_is_refused() {
        assert!(occurrences("{", "openEHR-EHR-EVALUATION.e.v1").is_err());
    }
}
