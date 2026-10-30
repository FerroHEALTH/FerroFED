// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A form's fields as posted.
//!
//! A form whose rows are keyed on the page (the query console's parameters,
//! the configuration's endpoints, settings and secrets) needs every field
//! by its whole name, in the order posted, so [`Fields`] reads the body a
//! browser posts (`application/x-www-form-urlencoded`, the URL Standard,
//! § 5.1) into its pairs as they stand. A field the page posts more than
//! once is named with `[]`, and a row's field in one step with dots
//! (`form[query.0.url]`); the `[]` is the page's mark of a repeated name
//! and is read off, so `form[query_rows][]` is the field `form[query_rows]`.
//! No specification governs this: our own design.

use std::fmt;

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

    /// The fields of the form body `body`, as a browser posts it, each
    /// repeated name read without its `[]`.
    #[must_use]
    pub fn read(body: &[u8]) -> Self {
        Self(
            url::form_urlencoded::parse(body)
                .map(|(name, value)| {
                    let name = name.strip_suffix("[]").unwrap_or(&name).to_owned();
                    (name, value.into_owned())
                })
                .collect(),
        )
    }

    /// The fields, each its whole name and its value.
    #[must_use]
    pub fn pairs(&self) -> &[(String, String)] {
        &self.0
    }

    /// The value of the field `name`, the last one posted, as posted.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .rev()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

impl fmt::Debug for Fields {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Fields")
            .field("count", &self.0.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::Fields;

    #[test]
    fn a_plain_post_reads_back_every_field_by_its_whole_name() {
        let fields = Fields::read(
            b"form%5Baction%5D=save&form%5Bquery_rows%5D%5B%5D=b2&form%5Bquery_rows%5D%5B%5D=a1\
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
        assert_eq!(Some("aql"), fields.get("kind"));
    }

    #[test]
    fn the_fields_cross_from_the_browser_to_the_server_and_back_whole() {
        let posted = Fields::new(vec![
            (String::from("form[action]"), String::from("save")),
            (String::from("form[query_rows]"), String::from("b2")),
            (String::from("form[query_rows]"), String::from("a1")),
            (String::from("form[query.0.url]"), String::from("x")),
            (String::from("aql"), String::from("SELECT 1")),
        ]);
        let mut wire = url::form_urlencoded::Serializer::new(String::new());
        for (name, value) in posted.pairs() {
            wire.append_pair(name, value);
        }
        let back = Fields::read(wire.finish().as_bytes());
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
