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
//!   context is given: the coding is kept as recorded and displayed with the
//!   term's text, which also opens the narrative.
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
use openehr_rm::v1_2::data_types::text::code_phrase::CodePhrase;
use openehr_rm::v1_2::data_types::text::dv_text::DvText;

use crate::terminology::Terminology;
use crate::terminology::TerminologyError;
use crate::terminology::ehn;
use crate::terminology::local::Term;
use crate::terminology::local::TermDefinitions;

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
    /// system or rendered from the template, so Art 10(7) applied.
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
    /// The terminology server gave no answer.
    #[error("the terminology server gave no translation")]
    Terminology(#[from] TerminologyError),
}

/// Transcodes `text` under `preferred`, with no template context, so a
/// `local` code follows Art 10(7).
///
/// # Errors
///
/// [`TranscodeError::InvalidMatch`] for a mapping whose `match` the RM does
/// not admit, and [`TranscodeError::Uncoded`] for a code phrase with no
/// terminology or no code.
// TODO(#926): apply these rules to each composition the summary maps through FHIRconnect.
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
/// language only follows Art 10(7), as [`transcode`] answers it.
///
/// # Errors
///
/// The errors of [`transcode`], and [`TranscodeError::UndefinedLocalCode`]
/// for a `local` code the archetype does not define in any language.
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
    let mut transcoded = Transcoded {
        narrative: vec![value.to_owned()],
        ..Transcoded::default()
    };
    if let Some(defining) = defining {
        match rendered(defining, local)? {
            Some(term) => {
                transcoded.codings.push(coding(defining, Some(term))?);
                transcoded.narrative = vec![term.to_owned()];
                if term != value {
                    transcoded.narrative.push(value.to_owned());
                }
                transcoded.from_template = true;
            }
            None => transcoded.codings.push(coding(defining, Some(value))?),
        }
    }
    for mapping in mappings.into_iter().flatten() {
        let relation = match mapping.r#match {
            '=' => {
                transcoded.codings.push(coding(
                    &mapping.target,
                    mapping.target.preferred_term.as_deref(),
                )?);
                continue;
            }
            '>' => Relation::Broader,
            '<' => Relation::Narrower,
            '?' => Relation::Unknown,
            _ => return Err(TranscodeError::InvalidMatch),
        };
        let target = &mapping.target;
        let mut line = format!(
            "{value}: {} {} {}",
            relation.words(),
            target.terminology_id.name(),
            target.code_string
        );
        if let Some(term) = &target.preferred_term {
            line.push_str(" (");
            line.push_str(term);
            line.push(')');
        }
        transcoded.narrative.push(line);
    }
    transcoded.untranscoded = !transcoded.from_template
        && !transcoded.codings.is_empty()
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

/// Returns the coding a code phrase stands for, displayed with `display`.
///
/// The terminology id's name is the coding's system and its version the
/// coding's version (openEHR BASE, `TERMINOLOGY_ID`).
fn coding(phrase: &CodePhrase, display: Option<&str>) -> Result<Coding, TranscodeError> {
    let system = phrase.terminology_id.name();
    if system.is_empty() || phrase.code_string.is_empty() {
        return Err(TranscodeError::Uncoded);
    }
    let version = phrase.terminology_id.version_id();
    Ok(Coding {
        system: Some(Uri::from(system)),
        version: (!version.is_empty()).then(|| FhirString::from(version)),
        code: Some(Code::from(phrase.code_string.as_str())),
        display: display
            .filter(|display| !display.is_empty())
            .map(FhirString::from),
        ..Coding::default()
    })
}

/// Returns the system a coding carries.
fn system_of(coding: &Coding) -> Option<&str> {
    coding.system.as_ref()?.value.as_deref()
}
