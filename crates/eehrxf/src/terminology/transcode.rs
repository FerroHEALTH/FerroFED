// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An openEHR text item turned into FHIR codings and narrative (feature
//! `openehr`).
//!
//! The rules, each from its source:
//!
//! - A `DV_CODED_TEXT` whose `defining_code` is in a preferred system passes
//!   as recorded: one coding with that system and code, displayed with the
//!   item's `value`, the rubric of the term.
//! - Each RM `TERM_MAPPING` whose `match` is `'='`, "a (supposedly)
//!   equivalent to the original item", adds a coding after the original
//!   (RM Release 1.1.0, Data Types IM, `TERM_MAPPING`,
//!   <https://specifications.openehr.org/releases/RM/Release-1.1.0/data_types.html#_term_mapping_class>).
//! - A mapping whose `match` is `'>'` (broader), `'<'` (narrower) or `'?'`
//!   (unknown) adds no coding, because a FHIR coding asserts the concept
//!   itself; it is written into the narrative.
//! - The item's text is always in the narrative: "all clinically relevant
//!   information in coded elements must also be included and visible in a
//!   narrative part" (eHealth Network Guidelines on Patient Summary, Release
//!   3.4, Art 10(3)).
//! - A code of the `local` terminology is rendered from the term definitions
//!   of the operational template, in the language asked, when a [`Local`]
//!   context is given: the coding carries the code under the system of the
//!   archetype it is defined in, displayed with the term's text, which also
//!   opens the narrative.
//! - A coding's system is the FHIR system URI of its terminology id
//!   ([`fhir_system`]). A code whose terminology has no known system, a
//!   `local` code with no template context among them, is written as no
//!   coding: its code goes into the narrative, [`Transcoded::unmapped`]
//!   names it, and the item follows Art 10(7).
//! - A coding's version is the terminology id's version in the form that
//!   system's versions take ([`fhir_version`]): a SNOMED CT version that
//!   names no edition, and an HL7 version that is no dotted number, is
//!   written as none.
//! - When no coding is in a preferred system, the information "should, as an
//!   interim solution, be transferred encoded, preferably displayed in
//!   English, and/or in narrative form" (Art 10(7)): the original coding is
//!   kept as recorded with its display, the text is in the narrative, and
//!   the result says it was not transcoded.
//!
//! [`transcode_with`] asks a configured terminology server's `$translate`
//! for each preferred system before Art 10(7) applies, and takes only an
//! `equivalent` or `equal` match.
use std::collections::BTreeSet;

use fhir_types::r4::coding::Coding;
use fhir_types::r4::primitives::Code;
use fhir_types::r4::primitives::String as FhirString;
use fhir_types::r4::primitives::Uri;
use openehr_base::v1_3::base_types::definitions::definitions_impl::LOCAL_TERMINOLOGY_ID;
use openehr_base::v1_3::base_types::identification::archetype_id::ArchetypeId;
use openehr_base::v1_3::base_types::identification::lexical::IdError;
use openehr_rm::v1_2::data_types::text::code_phrase::CodePhrase;
use openehr_rm::v1_2::data_types::text::dv_text::DvText;

use crate::terminology::Terminology;
use crate::terminology::TerminologyError;
use crate::terminology::ehn;
use crate::terminology::local::Term;
use crate::terminology::local::TermDefinitions;
use crate::terminology::system::fhir_system;
use crate::terminology::system::fhir_version;

/// The code systems a deployment takes as preferred, by the system URI a
/// coding carries.
///
/// The default is every system of the eHealth Network Guidelines on Patient
/// Summary §4 that [`ehn::PATIENT_SUMMARY`] ships; [`PreferredSystems::new`]
/// names another set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreferredSystems {
    systems: BTreeSet<String>,
}

impl Default for PreferredSystems {
    fn default() -> Self {
        Self::new(ehn::PATIENT_SUMMARY)
    }
}

impl PreferredSystems {
    /// Returns the preferred systems `systems` names.
    #[must_use]
    pub fn new<I, S>(systems: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            systems: systems.into_iter().map(Into::into).collect(),
        }
    }

    /// Returns whether `system` is preferred.
    #[must_use]
    pub fn contains(&self, system: &str) -> bool {
        self.systems.contains(system)
    }

    /// Returns every preferred system, in order.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.systems.iter().map(String::as_str)
    }
}

/// The template context a `local` code is read in: the operational
/// template's term definitions, the archetype the item sits in, and the
/// language asked.
#[derive(Debug, Clone, Copy)]
pub struct Local<'a> {
    terms: &'a TermDefinitions,
    archetype: &'a ArchetypeId,
    language: &'a str,
}

impl<'a> Local<'a> {
    /// Returns the context of an item of `archetype`, read from `terms` in
    /// `language`, an ISO 639-1 code as the template states its own.
    #[must_use]
    pub const fn new(
        terms: &'a TermDefinitions,
        archetype: &'a ArchetypeId,
        language: &'a str,
    ) -> Self {
        Self {
            terms,
            archetype,
            language,
        }
    }
}

/// The relation a mapping other than `'='` states, written into the
/// narrative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Relation {
    /// `'>'`: the mapping is to a broader term.
    Broader,
    /// `'<'`: the mapping is to a narrower term.
    Narrower,
    /// `'?'`: the kind of mapping is unknown.
    Unknown,
}

impl Relation {
    /// Returns the relation of the `match` code `code`: `'>'`, `'<'` or
    /// `'?'`, and `None` for `'='` or any other code.
    #[must_use]
    pub const fn of(code: char) -> Option<Self> {
        match code {
            '>' => Some(Self::Broader),
            '<' => Some(Self::Narrower),
            '?' => Some(Self::Unknown),
            _ => None,
        }
    }

    /// Returns the words the narrative names the relation with.
    #[must_use]
    pub const fn words(self) -> &'static str {
        match self {
            Self::Broader => "broader term",
            Self::Narrower => "narrower term",
            Self::Unknown => "related term of unknown kind",
        }
    }
}

/// An openEHR text item as FHIR codings and narrative.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Transcoded {
    codings: Vec<Coding>,
    narrative: Vec<String>,
    untranscoded: bool,
    from_template: bool,
    unmapped: Vec<Unmapped>,
}

/// A code written as no coding, because its terminology has no known FHIR
/// system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unmapped {
    terminology: String,
    code: String,
}

impl Unmapped {
    /// Returns the name of the code's openEHR terminology id.
    #[must_use]
    pub fn terminology(&self) -> &str {
        &self.terminology
    }

    /// Returns the code.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }
}

impl Transcoded {
    /// Returns the codings, the original first, then each equivalent one in
    /// mapping order.
    #[must_use]
    pub fn codings(&self) -> &[Coding] {
        &self.codings
    }

    /// Returns the narrative lines: the item's text, then one line per
    /// mapping no coding carries.
    ///
    /// For a `local` code rendered from the template, the first line is the
    /// term's text, followed by the recorded text when it differs.
    #[must_use]
    pub fn narrative(&self) -> &[String] {
        &self.narrative
    }

    /// Returns whether the item is coded and no coding is in a preferred
    /// system or rendered from the template, so Art 10(7) applied. An item
    /// whose every code is unmapped is untranscoded too.
    #[must_use]
    pub const fn untranscoded(&self) -> bool {
        self.untranscoded
    }

    /// Returns whether the item's `local` code was rendered from the
    /// template's term definitions.
    #[must_use]
    pub const fn is_from_template(&self) -> bool {
        self.from_template
    }

    /// Returns every code written as no coding because its terminology has
    /// no known FHIR system: the original first, then each `'='` mapping's.
    #[must_use]
    pub fn unmapped(&self) -> &[Unmapped] {
        &self.unmapped
    }
}

/// Why an item cannot be transcoded.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TranscodeError {
    /// A mapping's `match` is none of `'>'`, `'='`, `'<'` and `'?'`, which
    /// the RM invariant `Match_valid` refuses.
    #[error("a TERM_MAPPING carries a match outside '>', '=', '<' and '?'")]
    InvalidMatch,
    /// A code phrase names no terminology or no code.
    #[error("a CODE_PHRASE names no terminology or no code")]
    Uncoded,
    /// A `local` code the archetype the item sits in does not define in any
    /// language, so the item does not conform to its template.
    #[error("the archetype {archetype} defines no local code {code}")]
    UndefinedLocalCode {
        /// The archetype the item sits in.
        archetype: String,
        /// The code.
        code: String,
    },
    /// A `local` code is read in an archetype id that is not one, so no
    /// system URI can name the archetype.
    #[error("the archetype id {archetype} a local code is read in is not one")]
    ArchetypeId {
        /// The archetype id as given.
        archetype: String,
        /// Why it is not one.
        #[source]
        source: IdError,
    },
    /// The terminology server gave no answer.
    #[error("the terminology server gave no translation")]
    Terminology(#[from] TerminologyError),
}

/// Transcodes `text` under `preferred`, with no template context, so a
/// `local` code is unmapped and follows Art 10(7).
///
/// # Errors
///
/// [`TranscodeError::InvalidMatch`] for a mapping whose `match` the RM does
/// not admit, and [`TranscodeError::Uncoded`] for a code phrase with no
/// terminology or no code.
pub fn transcode(
    text: &DvText,
    preferred: &PreferredSystems,
) -> Result<Transcoded, TranscodeError> {
    rules(text, preferred, None)
}

/// Transcodes `text` under `preferred`, rendering a `local` code from the
/// template context `local`.
///
/// A `local` code the archetype defines in the language asked is displayed
/// with the term's text and is not untranscoded; one it defines in another
/// language only is a coding under the archetype's system, displayed as
/// recorded, and follows Art 10(7).
///
/// # Errors
///
/// The errors of [`transcode`], [`TranscodeError::UndefinedLocalCode`] for a
/// `local` code the archetype does not define in any language, and
/// [`TranscodeError::ArchetypeId`] for a context whose archetype id is not
/// one.
pub fn transcode_local(
    text: &DvText,
    preferred: &PreferredSystems,
    local: &Local<'_>,
) -> Result<Transcoded, TranscodeError> {
    rules(text, preferred, Some(local))
}

/// Transcodes `text` under `preferred`, asking `terminology` to translate an
/// item no coding of which is in a preferred system.
///
/// Each original coding is asked of `$translate` into each preferred
/// system, and every `equivalent` or `equal` match is added after the
/// codings the item carries. With no server configured, or no accepted
/// match, the item follows Art 10(7) as [`transcode`] answers it.
///
/// # Errors
///
/// The errors of [`transcode`], and [`TranscodeError::Terminology`] for a
/// `$translate` that gives no answer: a failed call is never taken as no
/// translation.
pub async fn transcode_with(
    text: &DvText,
    preferred: &PreferredSystems,
    terminology: &Terminology,
) -> Result<Transcoded, TranscodeError> {
    translated(transcode(text, preferred)?, preferred, terminology).await
}

/// Transcodes `text` under `preferred` and the template context `local`,
/// asking `terminology` to translate an item no coding of which is in a
/// preferred system or rendered from the template.
///
/// # Errors
///
/// The errors of [`transcode_local`], and [`TranscodeError::Terminology`]
/// for a `$translate` that gives no answer.
pub async fn transcode_local_with(
    text: &DvText,
    preferred: &PreferredSystems,
    local: &Local<'_>,
    terminology: &Terminology,
) -> Result<Transcoded, TranscodeError> {
    translated(
        transcode_local(text, preferred, local)?,
        preferred,
        terminology,
    )
    .await
}

/// Applies the rules to `text`, reading a `local` code in `local` when
/// given.
fn rules(
    text: &DvText,
    preferred: &PreferredSystems,
    local: Option<&Local<'_>>,
) -> Result<Transcoded, TranscodeError> {
    let (value, defining, mappings) = match text {
        DvText::DvCodedText(coded) => (
            coded.value.as_str(),
            Some(&coded.defining_code),
            coded.mappings.as_ref(),
        ),
        DvText::DvText(plain) => (plain.value.as_str(), None, plain.mappings.as_ref()),
    };
    let archetype = local.map(|local| local.archetype);
    let mut transcoded = Transcoded {
        narrative: vec![value.to_owned()],
        ..Transcoded::default()
    };
    if let Some(defining) = defining {
        let term = rendered(defining, local)?;
        if let Some(term) = term {
            transcoded.narrative = vec![term.to_owned()];
            if term != value {
                transcoded.narrative.push(value.to_owned());
            }
            transcoded.from_template = true;
        }
        let display = term.unwrap_or(value);
        transcoded.place(defining, Some(display), archetype, || {
            format!(
                "{value}: {} {}",
                defining.terminology_id.name(),
                defining.code_string
            )
        })?;
    }
    for mapping in mappings.into_iter().flatten() {
        let relation = match mapping.r#match {
            '=' => {
                let target = &mapping.target;
                transcoded.place(target, target.preferred_term.as_deref(), archetype, || {
                    narrated(value, "equivalent term", target)
                })?;
                continue;
            }
            '>' => Relation::Broader,
            '<' => Relation::Narrower,
            '?' => Relation::Unknown,
            _ => return Err(TranscodeError::InvalidMatch),
        };
        transcoded
            .narrative
            .push(narrated(value, relation.words(), &mapping.target));
    }
    transcoded.untranscoded = !transcoded.from_template
        && (!transcoded.codings.is_empty() || !transcoded.unmapped.is_empty())
        && !transcoded
            .codings
            .iter()
            .any(|coding| system_of(coding).is_some_and(|system| preferred.contains(system)));
    Ok(transcoded)
}

/// Returns the template's text for `defining` when it is a `local` code the
/// archetype of `local` defines in the language asked.
fn rendered<'a>(
    defining: &CodePhrase,
    local: Option<&Local<'a>>,
) -> Result<Option<&'a str>, TranscodeError> {
    let Some(local) = local else {
        return Ok(None);
    };
    if defining.terminology_id.name() != LOCAL_TERMINOLOGY_ID {
        return Ok(None);
    }
    match local
        .terms
        .term(local.language, local.archetype, &defining.code_string)
    {
        Term::Defined(text) => Ok(Some(text)),
        // NOTE: eHN PS Art 10(7): a term the template holds in another language only is
        // transferred encoded and displayed as recorded, with no language substituted.
        Term::OtherLanguage => Ok(None),
        Term::Undefined => Err(TranscodeError::UndefinedLocalCode {
            archetype: local.archetype.value.clone(),
            code: defining.code_string.clone(),
        }),
    }
}

/// Adds to `transcoded` every `equivalent` or `equal` translation
/// `terminology` gives of an untranscoded item into a preferred system.
async fn translated(
    mut transcoded: Transcoded,
    preferred: &PreferredSystems,
    terminology: &Terminology,
) -> Result<Transcoded, TranscodeError> {
    // NOTE: eHN PS Art 10(7) is the rule when no transcoding exists, and the server is
    // optional, so with none configured the item is transferred encoded with its narrative.
    if !transcoded.untranscoded || !terminology.is_configured() {
        return Ok(transcoded);
    }
    let originals: Vec<(String, String)> = transcoded
        .codings
        .iter()
        .filter_map(|coding| {
            let system = system_of(coding)?;
            let code = coding.code.as_ref()?.value.as_deref()?;
            Some((system.to_owned(), code.to_owned()))
        })
        .collect();
    let mut added = Vec::new();
    for (system, code) in &originals {
        for target in preferred.iter() {
            for found in terminology.translate(system, code, target).await? {
                if !transcoded.codings.contains(&found) && !added.contains(&found) {
                    added.push(found);
                }
            }
        }
    }
    if !added.is_empty() {
        transcoded.untranscoded = !added
            .iter()
            .any(|coding| system_of(coding).is_some_and(|system| preferred.contains(system)));
        transcoded.codings.extend(added);
    }
    Ok(transcoded)
}

impl Transcoded {
    /// Adds the coding `phrase` stands for, or, when its terminology has no
    /// known system, records it as unmapped with the narrative line `line`.
    fn place(
        &mut self,
        phrase: &CodePhrase,
        display: Option<&str>,
        archetype: Option<&ArchetypeId>,
        line: impl FnOnce() -> String,
    ) -> Result<(), TranscodeError> {
        if let Some(coding) = coding(phrase, display, archetype)? {
            self.codings.push(coding);
        } else {
            self.narrative.push(line());
            self.unmapped.push(Unmapped {
                terminology: phrase.terminology_id.name().to_owned(),
                code: phrase.code_string.clone(),
            });
        }
        Ok(())
    }
}

/// Returns the narrative line naming `target` as `words` of `value`.
fn narrated(value: &str, words: &str, target: &CodePhrase) -> String {
    let mut line = format!(
        "{value}: {words} {} {}",
        target.terminology_id.name(),
        target.code_string
    );
    if let Some(term) = &target.preferred_term {
        line.push_str(" (");
        line.push_str(term);
        line.push(')');
    }
    line
}

/// Returns the coding a code phrase stands for, displayed with `display`, a
/// `local` code read in `archetype`, or `None` when the phrase's terminology
/// has no known FHIR system.
///
/// The coding's system is the FHIR system URI of the terminology id's name
/// ([`fhir_system`]), and its version the id's version in the form that
/// system's versions take, or none ([`fhir_version`]).
fn coding(
    phrase: &CodePhrase,
    display: Option<&str>,
    archetype: Option<&ArchetypeId>,
) -> Result<Option<Coding>, TranscodeError> {
    let name = phrase.terminology_id.name();
    if name.is_empty() || phrase.code_string.is_empty() {
        return Err(TranscodeError::Uncoded);
    }
    let system = fhir_system(name, archetype).map_err(|source| TranscodeError::ArchetypeId {
        archetype: archetype.map_or_else(String::new, |archetype| archetype.value.clone()),
        source,
    })?;
    let Some(system) = system else {
        return Ok(None);
    };
    let version = fhir_version(&system, phrase.terminology_id.version_id()).map(FhirString::from);
    Ok(Some(Coding {
        system: Some(Uri::from(system)),
        version,
        code: Some(Code::from(phrase.code_string.as_str())),
        display: display
            .filter(|display| !display.is_empty())
            .map(FhirString::from),
        ..Coding::default()
    }))
}

/// Returns the system a coding carries.
fn system_of(coding: &Coding) -> Option<&str> {
    coding.system.as_ref()?.value.as_deref()
}
