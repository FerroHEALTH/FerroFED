// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The archetype-local terms of an operational template (feature
//! `openehr`).
//!
//! A `CODE_PHRASE` whose terminology is `local` names a code "local to the
//! knowledge resource in which it occurs, e.g. an archetype" (openEHR BASE
//! Release 1.1.0, `OPENEHR_DEFINITIONS.Local_terminology_id`): "the origin
//! of allowable values is described within the archetype" (openEHR RM
//! Release 1.1.0, Data Types IM, Text Package, Requirements). Its text is in
//! the term definitions the operational template carries for each archetype it
//! holds, so a local code is read in the context of the archetype the item
//! sits in.
//!
//! [`TermDefinitions`] keeps every term definition of an OPT 1.4 template,
//! read with `openehr-its`'s `opt14` model: the definitions of each
//! `C_ARCHETYPE_ROOT`, in the template's own language, and the
//! language-keyed definitions of its flattened ontologies.

use std::collections::BTreeMap;

use openehr_base::v1_3::base_types::identification::archetype_id::ArchetypeId;
use openehr_its::opt14::types::CArchetypeRoot;
use openehr_its::opt14::types::CAttribute;
use openehr_its::opt14::types::CObject;
use openehr_its::opt14::types::Codedefinitionset;
use openehr_its::opt14::types::OperationalTemplate;
use openehr_its::xml::runtime::XmlError;

/// The `items` key of an `ARCHETYPE_TERM` that holds the term's text.
const TEXT: &str = "text";

/// The term definitions of one operational template, by language, archetype
/// and code.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TermDefinitions {
    language: String,
    terms: BTreeMap<Key, String>,
}

/// A term's language, archetype id and code.
type Key = (String, String, String);

/// What the template defines for one local code in one language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Term<'a> {
    /// The term's text in the language asked.
    Defined(&'a str),
    /// The archetype defines the code, in another language only.
    OtherLanguage,
    /// The archetype does not define the code.
    Undefined,
}

/// Why an operational template's term definitions cannot be read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TermDefinitionsError {
    /// The text is not an OPT 1.4 operational template.
    #[error("the operational template is not an OPT 1.4 template")]
    Template(#[source] XmlError),
}

impl TermDefinitions {
    /// Reads the term definitions of the OPT 1.4 operational template `xml`.
    ///
    /// # Errors
    ///
    /// [`TermDefinitionsError::Template`] when `xml` is not an OPT 1.4
    /// operational template.
    pub fn from_opt14(xml: &str) -> Result<Self, TermDefinitionsError> {
        openehr_its::opt14::from_xml(xml)
            .map(|template| Self::from_template(&template))
            .map_err(TermDefinitionsError::Template)
    }

    /// Returns the term definitions of `template`.
    #[must_use]
    pub fn from_template(template: &OperationalTemplate) -> Self {
        let mut definitions = Self {
            language: template.language.code_string.clone(),
            terms: BTreeMap::new(),
        };
        definitions.root(&template.definition);
        for ontology in template
            .ontology
            .iter()
            .chain(&template.component_ontologies)
        {
            for set in &ontology.term_definitions {
                definitions.set(&ontology.archetype_id, set);
            }
            for set in &ontology.constraint_definitions {
                definitions.set(&ontology.archetype_id, set);
            }
        }
        definitions
    }

    /// Returns the template's own language, the language of the term
    /// definitions each archetype root carries.
    #[must_use]
    pub fn language(&self) -> &str {
        &self.language
    }

    /// Returns what the archetype `archetype` defines for `code` in
    /// `language`.
    #[must_use]
    pub fn term(&self, language: &str, archetype: &ArchetypeId, code: &str) -> Term<'_> {
        let archetype = archetype.value.as_str();
        let key = (language.to_owned(), archetype.to_owned(), code.to_owned());
        if let Some(text) = self.terms.get(&key) {
            return Term::Defined(text);
        }
        if self
            .terms
            .keys()
            .any(|(_, held, defined)| held == archetype && defined == code)
        {
            Term::OtherLanguage
        } else {
            Term::Undefined
        }
    }

    /// Keeps the term definitions of `root` and of every archetype root
    /// below it.
    fn root(&mut self, root: &CArchetypeRoot) {
        let archetype = root.archetype_id.value.as_str();
        let language = self.language.clone();
        for term in &root.term_definitions {
            if let Some(text) = term.items.get(TEXT) {
                self.insert(&language, archetype, &term.code, text);
            }
        }
        self.attributes(&root.attributes);
    }

    /// Walks `attributes` for the archetype roots they hold.
    fn attributes(&mut self, attributes: &[CAttribute]) {
        for attribute in attributes {
            let children = match attribute {
                CAttribute::CMultipleAttribute(multiple) => &multiple.children,
                CAttribute::CSingleAttribute(single) => &single.children,
            };
            for child in children {
                match child {
                    CObject::CArchetypeRoot(root) => self.root(root),
                    CObject::CComplexObject(complex) => self.attributes(&complex.attributes),
                    CObject::TComplexObject(complex) => self.attributes(&complex.attributes),
                    _ => {}
                }
            }
        }
    }

    /// Keeps the definitions of one language-keyed set of `archetype`.
    fn set(&mut self, archetype: &str, set: &Codedefinitionset) {
        for term in &set.items {
            if let Some(text) = term.items.get(TEXT) {
                self.insert(&set.language, archetype, &term.code, text);
            }
        }
    }

    /// Keeps one term, the first definition of a key winning.
    fn insert(&mut self, language: &str, archetype: &str, code: &str, text: &str) {
        self.terms
            .entry((language.to_owned(), archetype.to_owned(), code.to_owned()))
            .or_insert_with(|| text.to_owned());
    }
}
