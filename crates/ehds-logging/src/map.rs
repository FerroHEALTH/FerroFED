// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The deployment's category map: which categories the data under a template
//! or an archetype belong to (Regulation (EU) 2025/327 Annex II 3.2(c)).
//!
//! No adopted act and no openEHR specification ties a template or an
//! archetype to an Art 14(1) category, so the deployment declares it: each
//! template id and each archetype id maps to a set of categories, or to
//! `none`, the deployment's statement that the data under it are of no
//! category. The crate ships no map. A template key wins over an archetype
//! key for the same object, because a template is the clinical context the
//! archetype is used in (no specification governs the map: our own design).
//!
//! The map's [`CategoryMap::canonical`] text is the same for the same
//! declarations in any order, so a digest of it names the map a record was
//! classified under.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use serde::Deserialize;

use crate::category::{Category, CodeError, NationalCategory, Reference};

/// The word a declaration uses for data of no category.
pub const NONE: &str = "none";

/// One declaration as a configuration writes it: the word `none`, or a list
/// of categories.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Declared {
    /// A word, of which only [`NONE`] is admitted.
    Word(String),
    /// A list of categories, at least one: a priority category by its bare
    /// code or as `<system>|<code>`, a national one as `<system>|<code>`.
    Codes(Vec<String>),
}

/// What a key maps to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mapping {
    /// The data under the key belong to these categories.
    Categories(BTreeSet<Category>),
    /// The data under the key belong to no category.
    NoCategory,
}

/// The two tables a map keys on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Table {
    /// Template ids.
    Templates,
    /// Archetype ids.
    Archetypes,
}

impl Table {
    /// The table's name, as a configuration and an error name it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Templates => "templates",
            Self::Archetypes => "archetypes",
        }
    }
}

/// Why a map was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum MapError {
    /// A key is empty or holds a control character.
    #[error("a key of the {} table is empty or holds a control character", .0.name())]
    Key(Table),
    /// A key maps to a word other than `none`.
    #[error("{} key {key:?} maps to a word other than \"none\"", .table.name())]
    Word {
        /// The table.
        table: Table,
        /// The key.
        key: String,
    },
    /// A key maps to an empty list.
    #[error("{} key {key:?} maps to no category; write \"none\" for data of no category", .table.name())]
    Empty {
        /// The table.
        table: Table,
        /// The key.
        key: String,
    },
    /// A key maps to a code that is neither a priority category nor a
    /// declared national one.
    #[error("{} key {key:?} maps to {code:?}, which is neither a priority category nor a declared national one", .table.name())]
    Unknown {
        /// The table.
        table: Table,
        /// The key.
        key: String,
        /// The code.
        code: String,
    },
    /// A declared national category is refused.
    #[error("the national category {code:?} is refused")]
    National {
        /// The category as declared.
        code: String,
        /// Why.
        #[source]
        source: CodeError,
    },
}

/// The deployment's category map.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CategoryMap {
    national: BTreeSet<NationalCategory>,
    templates: BTreeMap<String, Mapping>,
    archetypes: BTreeMap<String, Mapping>,
    digest: Option<String>,
}

impl CategoryMap {
    /// The map `templates` and `archetypes` declare, their categories read
    /// against the six priority categories and the `national` ones, each
    /// declared `<system>|<code>`.
    ///
    /// # Errors
    ///
    /// A [`MapError`] for a refused national code, an empty or malformed
    /// key, a word other than `none`, an empty list, and a code that names
    /// no category.
    pub fn declare(
        national: &[String],
        templates: &BTreeMap<String, Declared>,
        archetypes: &BTreeMap<String, Declared>,
    ) -> Result<Self, MapError> {
        let national = national
            .iter()
            .map(|code| {
                code.parse::<NationalCategory>()
                    .map_err(|source| MapError::National {
                        code: code.clone(),
                        source,
                    })
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        let read = |table: Table, declared: &BTreeMap<String, Declared>| {
            declared
                .iter()
                .map(|(key, declared)| Ok((key.clone(), mapping(table, key, declared, &national)?)))
                .collect::<Result<BTreeMap<_, _>, MapError>>()
        };
        Ok(Self {
            templates: read(Table::Templates, templates)?,
            archetypes: read(Table::Archetypes, archetypes)?,
            national,
            digest: None,
        })
    }

    /// This map, named by `digest`, such as a hash of
    /// [`CategoryMap::canonical`].
    #[must_use]
    pub fn with_digest(mut self, digest: String) -> Self {
        self.digest = Some(digest);
        self
    }

    /// The digest the map is named by, when one was given.
    #[must_use]
    pub fn digest(&self) -> Option<&str> {
        self.digest.as_deref()
    }

    /// What `template_id` maps to, when the map holds it.
    #[must_use]
    pub fn template(&self, template_id: &str) -> Option<&Mapping> {
        self.templates.get(template_id)
    }

    /// What `archetype_id` maps to, when the map holds it.
    #[must_use]
    pub fn archetype(&self, archetype_id: &str) -> Option<&Mapping> {
        self.archetypes.get(archetype_id)
    }

    /// The category `reference` names: a priority category by its bare code
    /// or as `<system>|<code>`, or a national one the map declares, as
    /// `<system>|<code>`. Codes are compared exactly.
    #[must_use]
    pub fn category(&self, reference: &str) -> Option<Category> {
        named(reference, &self.national)
    }

    /// Whether the map holds no key.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.templates.is_empty() && self.archetypes.is_empty()
    }

    /// The map as text that is the same for the same declarations: one line
    /// per national category, then per template and per archetype key, each
    /// field separated by a tab, the categories by `,`, each category as
    /// `<system>|<code>`.
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut text = String::new();
        for national in &self.national {
            let _written = writeln!(text, "national\t{}|{}", national.system(), national.code());
        }
        for (table, keys) in [
            (Table::Templates, &self.templates),
            (Table::Archetypes, &self.archetypes),
        ] {
            for (key, mapping) in keys {
                let value = match mapping {
                    Mapping::NoCategory => NONE.to_owned(),
                    Mapping::Categories(categories) => categories
                        .iter()
                        .map(Category::token)
                        .collect::<Vec<_>>()
                        .join(","),
                };
                let _written = writeln!(text, "{}\t{key}\t{value}", table.name());
            }
        }
        text
    }
}

/// The mapping `declared` states for `key` of `table`.
fn mapping(
    table: Table,
    key: &str,
    declared: &Declared,
    national: &BTreeSet<NationalCategory>,
) -> Result<Mapping, MapError> {
    if key.is_empty() || key.chars().any(char::is_control) {
        return Err(MapError::Key(table));
    }
    let codes = match declared {
        Declared::Word(word) if word == NONE => return Ok(Mapping::NoCategory),
        Declared::Word(_) => {
            return Err(MapError::Word {
                table,
                key: key.to_owned(),
            });
        }
        Declared::Codes(codes) if codes.is_empty() => {
            return Err(MapError::Empty {
                table,
                key: key.to_owned(),
            });
        }
        Declared::Codes(codes) => codes,
    };
    codes
        .iter()
        .map(|code| {
            named(code, national).ok_or_else(|| MapError::Unknown {
                table,
                key: key.to_owned(),
                code: code.clone(),
            })
        })
        .collect::<Result<BTreeSet<_>, _>>()
        .map(Mapping::Categories)
}

/// The category `reference` names among the priority categories and the
/// `national` ones.
fn named(reference: &str, national: &BTreeSet<NationalCategory>) -> Option<Category> {
    let reference = Reference::read(reference);
    reference.priority().or_else(|| {
        national
            .iter()
            .find(|declared| reference.names(declared))
            .map(|declared| Category::National(declared.clone()))
    })
}
