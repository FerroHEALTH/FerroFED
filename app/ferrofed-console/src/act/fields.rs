// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A form's fields as posted, for a server function whose form has rows.
//!
//! A server function behind `<ActionForm>` reads its arguments with
//! `serde_qs` (`server_fn` 0.8, the `PostUrl` encoding), which nests
//! `a[b][c]` into maps and `a[]` into a sequence and refuses a name posted
//! twice. A form whose rows are keyed on the page (the query console's
//! parameters, the configuration's endpoints, settings and secrets) needs
//! every field by its whole name, in the order posted, so [`Fields`] walks
//! that nesting back into names: a map's entries as `name[key]`, a
//! sequence's items each under the name itself. A field posted more than
//! once is named with `[]` on the page, and a row's field is named in one
//! step with dots (`form[query.0.url]`), since `serde_qs` reads a step of
//! digits (`a[0]`) as the first item of a sequence. No specification
//! governs this: our own design.

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeMap as _, SerializeSeq as _, Serializer};
use serde::{Deserialize, Serialize};

/// Every field of a form by its whole name, in the order the form holds
/// them, each value as posted.
///
/// Its `Debug` output counts the fields and names none of their values,
/// since a value can name a patient (N33).
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Fields(Vec<(String, String)>);

impl Fields {
    /// The fields `pairs` hold, as a test or a plain post names them.
    #[must_use]
    pub fn new(pairs: Vec<(String, String)>) -> Self {
        Self(pairs)
    }

    /// The fields, each its whole name and its value.
    #[must_use]
    pub fn pairs(&self) -> &[(String, String)] {
        &self.0
    }
}

impl fmt::Debug for Fields {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Fields")
            .field("count", &self.0.len())
            .finish()
    }
}

/// One step of a name: `form[query][key]` is `form`, then `query`, then
/// `key`.
fn steps(name: &str) -> Vec<&str> {
    let (head, rest) = name.split_once('[').unwrap_or((name, ""));
    let mut steps = vec![head];
    if !rest.is_empty() {
        steps.extend(
            rest.trim_end_matches(']')
                .split("][")
                .filter(|step| !step.is_empty()),
        );
    }
    steps
}

/// A field's place in the nesting `serde_qs` writes and reads: the values
/// a name holds, or the names under it.
#[derive(Debug, Default)]
struct Node {
    /// The values posted under the name itself.
    values: Vec<String>,
    /// The names under it, by their next step.
    children: BTreeMap<String, Node>,
}

impl Node {
    /// Puts `value` at the end of the name `steps` spell under this node.
    fn put(&mut self, steps: &[&str], value: &str) {
        match steps.split_first() {
            None => self.values.push(value.to_owned()),
            Some((step, rest)) => self
                .children
                .entry((*step).to_owned())
                .or_default()
                .put(rest, value),
        }
    }
}

impl Serialize for Node {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.children.is_empty() {
            let mut seq = serializer.serialize_seq(Some(self.values.len()))?;
            for value in &self.values {
                seq.serialize_element(value)?;
            }
            return seq.end();
        }
        let mut map = serializer.serialize_map(Some(self.children.len()))?;
        for (step, child) in &self.children {
            map.serialize_entry(step, child)?;
        }
        map.end()
    }
}

impl Serialize for Fields {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut root = Node::default();
        for (name, value) in &self.0 {
            root.put(&steps(name), value);
        }
        let mut map = serializer.serialize_map(Some(root.children.len()))?;
        for (step, child) in &root.children {
            map.serialize_entry(step, child)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Fields {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut out = Vec::new();
        deserializer.deserialize_map(Walk {
            name: String::new(),
            out: &mut out,
        })?;
        Ok(Self(out))
    }
}

/// Walks one value of the nesting under `name`, writing each field it
/// holds into `out`.
struct Walk<'a> {
    /// The whole name so far.
    name: String,
    /// The fields read so far.
    out: &'a mut Vec<(String, String)>,
}

impl Walk<'_> {
    /// The name of the entry `key` under this one.
    fn under(&self, key: &str) -> String {
        if self.name.is_empty() {
            key.to_owned()
        } else {
            format!("{}[{key}]", self.name)
        }
    }
}

impl<'de> Visitor<'de> for Walk<'_> {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the fields of a form")
    }

    fn visit_str<E>(self, value: &str) -> Result<(), E> {
        self.out.push((self.name, value.to_owned()));
        Ok(())
    }

    fn visit_string<E>(self, value: String) -> Result<(), E> {
        self.out.push((self.name, value));
        Ok(())
    }

    fn visit_unit<E>(self) -> Result<(), E> {
        self.out.push((self.name, String::new()));
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        while let Some(key) = map.next_key::<String>()? {
            let name = self.under(&key);
            map.next_value_seed(Walk {
                name,
                out: &mut *self.out,
            })?;
        }
        Ok(())
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        while seq
            .next_element_seed(Walk {
                name: self.name.clone(),
                out: &mut *self.out,
            })?
            .is_some()
        {}
        Ok(())
    }
}

impl<'de> DeserializeSeed<'de> for Walk<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_any(self)
    }
}

#[cfg(test)]
mod tests {
    use super::Fields;

    fn read(query: &str) -> Fields {
        serde_qs::Config::new(5, false)
            .deserialize_str::<Fields>(query)
            .expect("the fields read")
    }

    #[test]
    fn a_plain_post_reads_back_every_field_by_its_whole_name() {
        let fields = read(
            "form%5Baction%5D=save&form%5Bquery_rows%5D%5B%5D=b2&form%5Bquery_rows%5D%5B%5D=a1\
             &form%5Bquery.b2.url%5D=http%3A%2F%2Fx&kind=aql&endpoint%5B%5D=n1\
             &endpoint%5B%5D=n2&form%5Bsecret.0.value%5D=a%2Bb%26c",
        );
        let named = |name: &str| -> Vec<&str> {
            fields
                .pairs()
                .iter()
                .filter(|(key, _)| key == name)
                .map(|(_, value)| value.as_str())
                .collect()
        };
        assert_eq!(vec!["b2", "a1"], named("form[query_rows]"));
        assert_eq!(vec!["n1", "n2"], named("endpoint"));
        assert_eq!(vec!["http://x"], named("form[query.b2.url]"));
        assert_eq!(vec!["a+b&c"], named("form[secret.0.value]"));
        assert_eq!(vec!["save"], named("form[action]"));
    }

    #[test]
    fn the_fields_cross_to_the_server_and_back_whole() {
        let posted = Fields::new(vec![
            (String::from("form[action]"), String::from("save")),
            (String::from("form[query_rows]"), String::from("b2")),
            (String::from("form[query_rows]"), String::from("a1")),
            (String::from("form[query.0.url]"), String::from("x")),
            (String::from("aql"), String::from("SELECT 1")),
        ]);
        let wire = serde_qs::to_string(&posted).expect("the fields write");
        let back = read(&wire);
        let mut sent = posted.pairs().to_vec();
        let mut read_back = back.pairs().to_vec();
        sent.sort();
        read_back.sort();
        assert_eq!(sent, read_back);
        assert_eq!(
            vec!["b2", "a1"],
            back.pairs()
                .iter()
                .filter(|(key, _)| key == "form[query_rows]")
                .map(|(_, value)| value.as_str())
                .collect::<Vec<_>>(),
            "a name's values keep their order"
        );
    }

    #[test]
    fn the_debug_output_names_no_value() {
        let fields = Fields::new(vec![(String::from("aql"), String::from("Qz7sentinel"))]);
        assert!(!format!("{fields:?}").contains("Qz7sentinel"));
    }
}
