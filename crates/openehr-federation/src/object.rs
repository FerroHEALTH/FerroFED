// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Open JSON objects: the members a type models, and the unknown members it
//! carries through unchanged.
//!
//! Every federation object in the two schemas is `additionalProperties: true`
//! by design, so a reader keeps the members it does not know. They are held as
//! the raw JSON text they arrived with ([`RawValue`]), which keeps their bytes
//! and keeps an untyped JSON tree out of the model. A reader refuses a JSON
//! object that names one member twice, because its value is then ambiguous.
//!
//! The wire format is JSON text: the readers here require `serde_json`'s
//! deserializer (`serde_json::from_str`, `from_slice` or `from_reader`).

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{DeserializeOwned, Deserializer, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::value::RawValue;

use crate::error::WireError;

/// The members of an open object that its type does not model, kept as the
/// raw JSON text they arrived with and written back in name order.
#[derive(Debug, Clone, Default)]
pub struct Extra(BTreeMap<String, Box<RawValue>>);

impl Extra {
    /// An empty set of extra members.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the object carried no member beyond the modelled ones.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The number of extra members.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// The raw JSON value of the extra member `name`, if present.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&RawValue> {
        self.0.get(name).map(AsRef::as_ref)
    }

    /// The extra members in name order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &RawValue)> {
        self.0
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_ref()))
    }

    /// Adds the extra member `name`, returning the value it replaces.
    ///
    /// A name the owning type models is refused when the object is written,
    /// with [`WireError::ExtraShadowsMember`].
    pub fn insert(
        &mut self,
        name: impl Into<String>,
        value: Box<RawValue>,
    ) -> Option<Box<RawValue>> {
        self.0.insert(name.into(), value)
    }

    /// Adds the extra member `name` with `value` serialized to JSON text.
    ///
    /// # Errors
    ///
    /// Returns [`WireError::Envelope`] when `value` cannot be serialized.
    pub fn insert_serialized<T: Serialize + ?Sized>(
        &mut self,
        name: impl Into<String>,
        value: &T,
    ) -> Result<Option<Box<RawValue>>, WireError> {
        let raw = serde_json::value::to_raw_value(value).map_err(WireError::Envelope)?;
        Ok(self.insert(name, raw))
    }

    /// Writes the extra members after the modelled ones, refusing a name the
    /// owning type models.
    pub(crate) fn write<M: SerializeMap>(
        &self,
        map: &mut M,
        object: &'static str,
        modelled: &[&str],
    ) -> Result<(), M::Error> {
        for (name, value) in &self.0 {
            if modelled.contains(&name.as_str()) {
                return Err(serde::ser::Error::custom(WireError::ExtraShadowsMember {
                    object,
                    member: name.clone(),
                }));
            }
            map.serialize_entry(name, value)?;
        }
        Ok(())
    }
}

impl PartialEq for Extra {
    fn eq(&self, other: &Self) -> bool {
        self.0.len() == other.0.len()
            && self
                .0
                .iter()
                .zip(&other.0)
                .all(|((name, value), (other_name, other_value))| {
                    name == other_name && value.get() == other_value.get()
                })
    }
}

impl Eq for Extra {}

/// The members of one JSON object, read once and taken out by name.
pub(crate) struct Members {
    object: &'static str,
    members: BTreeMap<String, Box<RawValue>>,
}

impl Members {
    /// Reads a JSON object, refusing a member named twice.
    pub(crate) fn read<'de, D: Deserializer<'de>>(
        object: &'static str,
        deserializer: D,
    ) -> Result<Self, D::Error> {
        deserializer.deserialize_map(MembersVisitor { object })
    }

    /// Takes the member `name` out, parsed as `T`, if it is present.
    ///
    /// A JSON `null` is malformed for every member the schemas define, so it
    /// is refused rather than read as absent.
    pub(crate) fn optional<T: DeserializeOwned>(
        &mut self,
        name: &'static str,
    ) -> Result<Option<T>, WireError> {
        self.members
            .remove(name)
            .map(|raw| {
                serde_json::from_str(raw.get()).map_err(|source| WireError::MalformedMember {
                    object: self.object,
                    member: name,
                    source,
                })
            })
            .transpose()
    }

    /// Takes the required member `name` out, parsed as `T`.
    pub(crate) fn required<T: DeserializeOwned>(
        &mut self,
        name: &'static str,
    ) -> Result<T, WireError> {
        self.optional(name)?.ok_or(WireError::MissingMember {
            object: self.object,
            member: name,
        })
    }

    /// What is left after every modelled member was taken out.
    pub(crate) fn into_extra(self) -> Extra {
        Extra(self.members)
    }
}

struct MembersVisitor {
    object: &'static str,
}

impl<'de> Visitor<'de> for MembersVisitor {
    type Value = Members;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "the JSON object `{}`", self.object)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Members, A::Error> {
        let mut members = BTreeMap::new();
        while let Some(name) = map.next_key::<String>()? {
            if members.contains_key(&name) {
                return Err(serde::de::Error::custom(WireError::DuplicateMember {
                    object: self.object,
                    member: name,
                }));
            }
            let value: Box<RawValue> = map.next_value()?;
            members.insert(name, value);
        }
        Ok(Members {
            object: self.object,
            members,
        })
    }
}

/// An absolute URI, the `format: uri` of the schemas, kept as written.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Uri(String);

impl Uri {
    /// Checks that `text` is an absolute URI.
    ///
    /// # Errors
    ///
    /// Returns [`WireError::InvalidUri`] when `text` does not parse as an
    /// absolute URI.
    pub fn new(text: impl Into<String>) -> Result<Self, WireError> {
        let text = text.into();
        url::Url::parse(&text).map_err(|source| WireError::InvalidUri {
            object: "uri",
            member: "uri",
            source,
        })?;
        Ok(Self(text))
    }

    /// The URI as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for Uri {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Uri {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::new(text).map_err(serde::de::Error::custom)
    }
}

/// Implements `Serialize` and `Deserialize` for a plain open record: every
/// field is a member of the same name, written in declaration order, followed
/// by the record's `extra` members.
macro_rules! plain_record {
    ($ty:ident, $object:literal, { $($field:ident : $obligation:ident),* $(,)? }) => {
        impl $ty {
            /// The member names this type models, as the schema spells them.
            pub const MEMBERS: &'static [&'static str] = &[$(stringify!($field)),*];
        }

        impl serde::Serialize for $ty {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                use serde::ser::SerializeMap as _;
                let mut map = serializer.serialize_map(None)?;
                $( plain_record!(@write map, self, $field, $obligation); )*
                self.extra.write(&mut map, $object, Self::MEMBERS)?;
                map.end()
            }
        }

        impl<'de> serde::Deserialize<'de> for $ty {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let mut members = $crate::object::Members::read($object, deserializer)?;
                let record = Self {
                    $( $field: plain_record!(@read members, $field, $obligation)
                        .map_err(serde::de::Error::custom)?, )*
                    extra: members.into_extra(),
                };
                Ok(record)
            }
        }
    };
    (@write $map:ident, $record:ident, $field:ident, required) => {
        $map.serialize_entry(stringify!($field), &$record.$field)?;
    };
    (@write $map:ident, $record:ident, $field:ident, optional) => {
        if let Some(value) = &$record.$field {
            $map.serialize_entry(stringify!($field), value)?;
        }
    };
    (@read $members:ident, $field:ident, required) => {
        $members.required(stringify!($field))
    };
    (@read $members:ident, $field:ident, optional) => {
        $members.optional(stringify!($field))
    };
}
