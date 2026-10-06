// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! How long an access record is kept (Regulation (EU) 2025/327 Art 9(2),
//! Annex II 3.4).
//!
//! Art 9(2) keeps the information on each access "available for at least
//! three years from each date of access", and Annex II 3.4 asks for
//! "different retention periods ... that take into account the origins and
//! categories" of the data. A [`RetentionPolicy`] declares a period in whole
//! years for every record, per category and per origin, none under
//! [`FLOOR_YEARS`]. A record is kept for the longest period its categories
//! and origins call for, and an access the map could not classify for the
//! longest the policy declares anywhere, so no record is kept for less than
//! one of its parts needs (no specification governs how origin and category
//! combine: our own design).
//!
//! The store that holds the records applies the period: the record states it
//! as a [`Retention`], with the first date it may be deleted.

use std::collections::BTreeMap;

use jiff::civil::Date;
use jiff::tz::Offset;
use jiff::{Span, Timestamp};

use crate::category::Category;
use crate::classify::Classification;
use crate::map::CategoryMap;

/// The fewest years a record is kept: "at least three years from each date
/// of access" (Art 9(2)).
pub const FLOOR_YEARS: u16 = 3;

/// A retention period in whole years, never under [`FLOOR_YEARS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Years(u16);

impl Years {
    /// The floor of Art 9(2), three years.
    pub const FLOOR: Self = Self(FLOOR_YEARS);

    /// The period of `years`, when it is not under the floor.
    #[must_use]
    pub const fn new(years: u16) -> Option<Self> {
        if years < FLOOR_YEARS {
            None
        } else {
            Some(Self(years))
        }
    }

    /// The number of years.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// What called for a record's period.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Ground {
    /// The period every record is kept for.
    Default,
    /// The access is unclassified, so it is kept for the longest period
    /// declared anywhere.
    Unclassified,
    /// One of the categories of the data accessed.
    Category(Category),
    /// One of the origins of the data, by its endpoint.
    Origin(String),
}

impl Ground {
    /// The ground as a record writes it: `default`, `unclassified`,
    /// `category:<system>|<code>` or `origin:<endpoint>`.
    #[must_use]
    pub fn code(&self) -> String {
        match self {
            Self::Default => "default".to_owned(),
            Self::Unclassified => "unclassified".to_owned(),
            Self::Category(category) => format!("category:{}", category.token()),
            Self::Origin(endpoint) => format!("origin:{endpoint}"),
        }
    }
}

/// How long one record is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retention {
    years: Years,
    ground: Ground,
    ends: Date,
}

impl Retention {
    /// The period.
    #[must_use]
    pub fn years(&self) -> Years {
        self.years
    }

    /// What called for it.
    #[must_use]
    pub fn ground(&self) -> &Ground {
        &self.ground
    }

    /// The first date, in UTC, on which the record may be deleted: the date
    /// of access plus the period, plus one day.
    #[must_use]
    pub fn ends(&self) -> Date {
        self.ends
    }
}

/// Why a retention policy was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RetentionError {
    /// A period is under the floor of Art 9(2).
    #[error(
        "{key} keeps a record {years} years, under the {FLOOR_YEARS} years from each date of access Regulation (EU) 2025/327 Art 9(2) asks for"
    )]
    UnderFloor {
        /// The key that declares it: `years`, `categories.<code>` or
        /// `origins.<endpoint>`.
        key: String,
        /// The period declared.
        years: u16,
    },
    /// A category key is neither a priority category nor a national one
    /// the category map declares.
    #[error("categories key {code:?} is neither a priority category nor a declared national one")]
    UnknownCategory {
        /// The code.
        code: String,
    },
    /// An origin key is empty or holds a control character.
    #[error("an origins key is empty or holds a control character")]
    OriginKey,
}

/// The periods a deployment keeps its records for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetentionPolicy {
    default: Years,
    categories: BTreeMap<Category, Years>,
    origins: BTreeMap<String, Years>,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            default: Years::FLOOR,
            categories: BTreeMap::new(),
            origins: BTreeMap::new(),
        }
    }
}

impl RetentionPolicy {
    /// The policy that keeps every record `years`, the data of each category
    /// of `categories` and from each endpoint of `origins` as long as they
    /// say, each category read as [`CategoryMap::category`] reads it.
    ///
    /// # Errors
    ///
    /// A [`RetentionError`] for a period under [`FLOOR_YEARS`], a code that
    /// names no category, and an empty or malformed endpoint.
    pub fn declare(
        years: u16,
        categories: &BTreeMap<String, u16>,
        origins: &BTreeMap<String, u16>,
        map: &CategoryMap,
    ) -> Result<Self, RetentionError> {
        let floor = |key: String, years: u16| {
            Years::new(years).ok_or(RetentionError::UnderFloor { key, years })
        };
        let default = floor("years".to_owned(), years)?;
        let categories = categories
            .iter()
            .map(|(code, years)| {
                let category = map
                    .category(code)
                    .ok_or_else(|| RetentionError::UnknownCategory { code: code.clone() })?;
                Ok((category, floor(format!("categories.{code}"), *years)?))
            })
            .collect::<Result<BTreeMap<_, _>, RetentionError>>()?;
        let origins = origins
            .iter()
            .map(|(endpoint, years)| {
                if endpoint.is_empty() || endpoint.chars().any(char::is_control) {
                    return Err(RetentionError::OriginKey);
                }
                Ok((
                    endpoint.clone(),
                    floor(format!("origins.{endpoint}"), *years)?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>, RetentionError>>()?;
        Ok(Self {
            default,
            categories,
            origins,
        })
    }

    /// The endpoints the policy names.
    pub fn origins(&self) -> impl Iterator<Item = &str> {
        self.origins.keys().map(String::as_str)
    }

    /// The longest period the policy declares anywhere.
    #[must_use]
    pub fn longest(&self) -> Years {
        self.categories
            .values()
            .chain(self.origins.values())
            .copied()
            .fold(self.default, Years::max)
    }

    /// How long the record of an access made at `recorded`, classified as
    /// `classified`, from the endpoints `origins`, is kept.
    ///
    /// The period is the longest of the default, each category's and each
    /// origin's, and the longest declared anywhere for an access that is
    /// unclassified in whole or in part. The ground is the first of these,
    /// in that order, that calls for the period.
    #[must_use]
    pub fn retain<'a>(
        &self,
        recorded: Timestamp,
        classified: &Classification,
        origins: impl IntoIterator<Item = &'a str>,
    ) -> Retention {
        let (years, ground) = if classified.unclassified().is_some() {
            (self.longest(), Ground::Unclassified)
        } else {
            let categories = classified.categories().keys().filter_map(|category| {
                self.categories
                    .get(category)
                    .map(|years| (*years, Ground::Category(category.clone())))
            });
            let origins = origins.into_iter().filter_map(|endpoint| {
                self.origins
                    .get(endpoint)
                    .map(|years| (*years, Ground::Origin(endpoint.to_owned())))
            });
            categories
                .chain(origins)
                .fold((self.default, Ground::Default), |kept, candidate| {
                    if candidate.0 > kept.0 {
                        candidate
                    } else {
                        kept
                    }
                })
        };
        Retention {
            years,
            ground,
            ends: ends(recorded, years),
        }
    }
}

/// The first UTC date a record of an access at `recorded` kept `years` may
/// be deleted on.
fn ends(recorded: Timestamp, years: Years) -> Date {
    let accessed = Offset::UTC.to_datetime(recorded).date();
    // NOTE: Art 9(2) sets a floor, so a date past the calendar's end keeps the record longer,
    // never shorter; the added day keeps a leap day's access its full period.
    Span::new()
        .try_years(i64::from(years.get()))
        .and_then(|span| span.try_days(1))
        .map_or(Date::MAX, |span| accessed.saturating_add(span))
}
