// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The locale the console renders in, and the catalog of its own words.
//!
//! The console's words are messages of the Fluent catalogs under the
//! crate's `locales/` directory, one directory per language and one file
//! per area of the console (`locales/en/<area>.ftl`), embedded at build time
//! by `fluent_templates::static_loader!`, the mechanism FerroUI catalogs its
//! own words with. English is the one catalog that ships and the one every
//! lookup falls back to; a language joins as one catalog directory and no
//! code change. Each area's messages are declared in `locale/<area>.rs`
//! beside its catalog file, so two changes to different areas touch
//! different files.
//!
//! The server chooses the locale once per request ([`requested`]): the
//! language the operator chose in Your preferences, else the best language
//! of the request's `Accept-Language` the catalog holds, else the
//! configured `[server] locale`. [`crate::app::App`] provides it with
//! `ferrohealth_ui::i18n::provide_locale`, so the console's words and the
//! components' words follow the same choice, and the document writes it as
//! `<html lang>` (WCAG 2.2 SC 3.1.1). The catalog is embedded in both
//! halves and the browser provides the locale the server chose
//! (`crate::visit::Visit`), so a page drawn in the browser speaks as the
//! server's does. A number
//! a message takes is grouped in the page's language by FerroUI's
//! formatter, with the catalog's `number-group` and `number-decimal`. No
//! specification governs the catalog layout or the choice: our own design.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::LazyLock;

use ferrohealth_ui::i18n::{Arg, Locale, Message, UnknownLanguage, library_catalog, locale};
use fluent_templates::fluent_bundle::FluentValue;
use fluent_templates::{LanguageIdentifier, Loader};

fluent_templates::static_loader! {
    static CONSOLE = {
        locales: "./locales",
        fallback_language: "en",
        // NOTE: Fluent's guide, "Bidirectionality": the isolation marks around a
        // placeable are for mixed-direction text; the English catalog draws none.
        customise: |bundle| {
            bundle.set_use_isolating(false);
            bundle.set_formatter(Some(ferrohealth_ui::i18n::number::format_number));
        },
    };
}

/// The tag of the pseudo-locale: a private-use language subtag (BCP 47,
/// `qaa` to `qtz`), so no catalog can serve it.
pub const PSEUDO_TAG: &str = "qps";

/// The pseudo-locale's language.
static PSEUDO: LazyLock<LanguageIdentifier> = LazyLock::new(|| fluent_templates::langid!("qps"));

/// The console's catalog: its own words in every language it ships, and in
/// the pseudo-locale the console's and FerroUI's English, stretched.
#[derive(Debug, Clone, Copy)]
pub struct Catalog;

/// The one catalog the console looks its words up in.
static CATALOG: Catalog = Catalog;

impl Loader for Catalog {
    fn lookup_complete(
        &self,
        lang: &LanguageIdentifier,
        text_id: &str,
        args: Option<&HashMap<Cow<'static, str>, FluentValue>>,
    ) -> String {
        self.try_lookup_complete(lang, text_id, args)
            .unwrap_or_else(|| text_id.to_owned())
    }

    fn try_lookup_complete(
        &self,
        lang: &LanguageIdentifier,
        text_id: &str,
        args: Option<&HashMap<Cow<'static, str>, FluentValue>>,
    ) -> Option<String> {
        if *lang != *PSEUDO {
            return CONSOLE.try_lookup_complete(lang, text_id, args);
        }
        let english = fluent_templates::langid!("en");
        CONSOLE
            .try_lookup_complete(&english, text_id, args)
            .or_else(|| library_catalog().try_lookup_complete(&english, text_id, args))
            .map(|english| stretch(&english))
    }

    fn locales(&self) -> Box<dyn Iterator<Item = &LanguageIdentifier> + '_> {
        CONSOLE.locales()
    }
}

/// The console's catalogs, every language it ships; the pseudo-locale is
/// served beside them and never offered.
#[must_use]
pub fn catalog() -> &'static (dyn Loader + Send + Sync) {
    &CATALOG
}

/// The locale `[server] locale` names: the pseudo-locale for its tag, over
/// the console's catalog, else the language `tag` names.
///
/// # Errors
/// Returns [`UnknownLanguage`] when `tag` is no language identifier.
pub fn configured(tag: &str) -> Result<Locale, UnknownLanguage> {
    let locale = Locale::parse(tag)?;
    Ok(if tag == PSEUDO_TAG {
        locale.with_catalog(&CATALOG)
    } else {
        locale
    })
}

/// `english` accented letter by letter and padded by two fifths of its
/// length (at least three), in brackets, so a cut end shows: the
/// pseudo-locale's words, as FerroUI's gallery draws its own.
#[must_use]
pub fn stretch(english: &str) -> String {
    let accented: String = english.chars().map(accent).collect();
    let length = english.chars().count();
    let pad = length.saturating_mul(2).div_ceil(5).max(3);
    format!("[{accented} {}]", "~".repeat(pad))
}

/// The accented look-alike of `letter`, the letter itself when it has none.
const fn accent(letter: char) -> char {
    match letter {
        'a' => '\u{e1}',
        'c' => '\u{e7}',
        'e' => '\u{e9}',
        'i' => '\u{ed}',
        'n' => '\u{f1}',
        'o' => '\u{f6}',
        'u' => '\u{fc}',
        'y' => '\u{fd}',
        'A' => '\u{c5}',
        'C' => '\u{c7}',
        'E' => '\u{c9}',
        'I' => '\u{ce}',
        'N' => '\u{d1}',
        'O' => '\u{d6}',
        'U' => '\u{dc}',
        'Y' => '\u{dd}',
        other => other,
    }
}

/// A string the console draws, by its message in the catalog. Each message
/// is an associated constant declared by its area's file under `locale/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Msg(&'static str);

impl Msg {
    /// Every message, area by area.
    pub fn all() -> impl Iterator<Item = Self> {
        AREAS.iter().flat_map(|area| area.iter().copied())
    }

    /// The message's id in the catalog.
    #[must_use]
    pub const fn id(self) -> &'static str {
        self.0
    }
}

/// Declares one area's messages: each constant names one message of the
/// catalog by its id, and the area's `ALL` lists them for [`AREAS`].
macro_rules! messages {
    ($($(#[$meta:meta])+ $name:ident = $id:literal,)+) => {
        #[expect(
            non_upper_case_globals,
            reason = "a message is named as a word of the page, Msg::Dashboard"
        )]
        impl super::Msg {
            $($(#[$meta])+ pub const $name: Self = Self($id);)+
        }

        /// The area's messages, in the order of their ids.
        pub(super) const ALL: &[super::Msg] = &[$(super::Msg::$name,)+];
    };
}

// The areas, one file each, kept in alphabetical order.
mod about;
mod act;
mod answers;
mod common;
mod configuration;
mod dashboard;
mod describe;
mod errors;
mod federation;
mod integrity;
mod prefs;
mod query;
mod search;
mod settings;
mod shell;
mod sign_in;
mod stored;
mod topology;

/// Every area's messages.
const AREAS: &[&[Msg]] = &[
    about::ALL,
    act::ALL,
    answers::ALL,
    common::ALL,
    configuration::ALL,
    dashboard::ALL,
    describe::ALL,
    errors::ALL,
    federation::ALL,
    integrity::ALL,
    prefs::ALL,
    query::ALL,
    search::ALL,
    settings::ALL,
    shell::ALL,
    sign_in::ALL,
    stored::ALL,
    topology::ALL,
];

/// The console's message `msg` in the page's locale.
#[must_use]
pub fn tr(msg: Msg) -> String {
    locale().lookup(catalog(), msg.id())
}

/// The console's message `msg` in the page's locale, with the placeables
/// `args` filled in.
#[must_use]
pub fn tr_with(msg: Msg, args: &[(&'static str, Arg)]) -> String {
    locale().lookup_with(catalog(), msg.id(), args)
}

/// The console's message `msg` in `locale`, with the placeables `args`
/// filled in: for a handler, where no page tree is in reach.
#[must_use]
pub fn tr_in(locale: &Locale, msg: Msg, args: &[(&'static str, Arg)]) -> String {
    locale.lookup_with(catalog(), msg.id(), args)
}

/// The console's message `msg` as a [`Message`] a component looks up as it
/// draws, such as a listing's noun.
#[must_use]
pub const fn message(msg: Msg) -> Message {
    Message::new(&CATALOG, msg.id())
}

/// The locales the console offers in Your preferences, each named in its
/// own language.
#[must_use]
pub fn choices() -> Vec<Locale> {
    let mut tags: Vec<String> = catalog().locales().map(ToString::to_string).collect();
    tags.sort();
    tags.iter()
        .filter_map(|tag| Locale::parse(tag).ok())
        .collect()
}

/// The locale a request is served in.
///
/// It is the language preference `chosen` when the catalog holds it, else
/// the best language of the `Accept-Language` `header` the catalog holds,
/// else `configured`.
#[must_use]
pub fn chosen(chosen: Option<&str>, header: &str, configured: Locale) -> Locale {
    let held = |tag: &str| catalog().locales().any(|language| *language == tag);
    if let Some(tag) = chosen.filter(|tag| held(tag))
        && let Ok(locale) = Locale::parse(tag)
    {
        return locale;
    }
    Locale::negotiate_with(header, catalog(), configured)
}

/// The locale of the request being served.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn requested() -> Locale {
    use leptos::context::use_context;
    let configured = use_context::<crate::server::ConsoleState>()
        .map_or_else(Locale::english, |state| state.settings().locale.clone());
    let parts = use_context::<http::request::Parts>();
    let header = parts
        .as_ref()
        .and_then(|parts| parts.headers.get(http::header::ACCEPT_LANGUAGE))
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let (cookies, prefix) = crate::visit::requested_preferences();
    let preferences = ferrohealth_ui::theme::Preferences::read(&cookies, &prefix);
    chosen(preferences.language.tag(), header, configured)
}

/// The locale the request's `Accept-Language` asks for among the shipped
/// catalogs, which Your preferences names as the browser's own.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn browser() -> Locale {
    use leptos::context::use_context;
    use_context::<http::request::Parts>()
        .and_then(|parts| {
            parts
                .headers
                .get(http::header::ACCEPT_LANGUAGE)
                .and_then(|value| value.to_str().ok())
                .map(Locale::negotiate)
        })
        .unwrap_or_default()
}

/// The locale of a request whose headers are `headers`, on the console
/// `state`: for a handler, where no page tree is in reach.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn requested_from(state: &crate::server::ConsoleState, headers: &http::HeaderMap) -> Locale {
    let header = headers
        .get(http::header::ACCEPT_LANGUAGE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let cookies = headers
        .get(http::header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let preferences =
        ferrohealth_ui::theme::Preferences::read(cookies, &state.sessions().preference_prefix());
    chosen(
        preferences.language.tag(),
        header,
        state.settings().locale.clone(),
    )
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::collections::HashMap;
    use std::path::Path;

    use ferrohealth_ui::i18n::Locale;
    use fluent_templates::fluent_bundle::FluentValue;

    use super::{Msg, catalog};

    fluent_templates::static_loader! {
        static THROWAWAY = {
            locales: "./tests/locales",
            fallback_language: "qaa",
            customise: |bundle| bundle.set_use_isolating(false),
        };
    }

    /// Every placeable the catalog files under `dir` name, each set to a
    /// number, so a message that selects a form by one formats whole.
    fn placeables(dir: &Path) -> HashMap<Cow<'static, str>, FluentValue<'static>> {
        let mut names = HashMap::new();
        for entry in std::fs::read_dir(dir).expect("a catalog directory") {
            let source =
                std::fs::read_to_string(entry.expect("an entry").path()).expect("a catalog file");
            for name in source
                .split("{ $")
                .skip(1)
                .filter_map(|rest| rest.split([' ', '}']).next())
            {
                names.insert(Cow::Owned(name.to_owned()), FluentValue::from(3));
            }
        }
        names
    }

    #[test]
    fn the_pseudo_locale_stretches_every_word_and_is_never_offered() {
        let args = placeables(&Path::new(env!("CARGO_MANIFEST_DIR")).join("locales/en"));
        let pseudo: fluent_templates::LanguageIdentifier =
            super::PSEUDO_TAG.parse().expect("the pseudo-locale's tag");
        let english: fluent_templates::LanguageIdentifier = "en".parse().expect("English");
        assert!(super::configured(super::PSEUDO_TAG).is_ok());
        for msg in Msg::all() {
            let plain = catalog()
                .try_lookup_with_args(&english, msg.id(), &args)
                .unwrap_or_default();
            let stretched = catalog()
                .try_lookup_with_args(&pseudo, msg.id(), &args)
                .unwrap_or_default();
            assert!(stretched.starts_with('['), "{}: {stretched}", msg.id());
            assert!(
                stretched.chars().count() > plain.chars().count(),
                "{}: {stretched}",
                msg.id()
            );
        }
        assert_eq!("[\u{c7}\u{f6}\u{f1}f\u{ed}g ~~~]", super::stretch("Config"));
        assert!(
            catalog()
                .locales()
                .all(|language| *language != super::PSEUDO_TAG),
            "the pseudo-locale is no language a request can choose"
        );
        assert!(
            super::choices()
                .iter()
                .all(|locale| locale.tag() != super::PSEUDO_TAG)
        );
    }

    #[test]
    fn every_catalog_directory_is_a_language_holding_every_message_and_no_other() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("locales");
        let mut found: Vec<String> = std::fs::read_dir(&root)
            .expect("the locales directory")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        found.sort();
        let mut shipped: Vec<String> = catalog().locales().map(ToString::to_string).collect();
        shipped.sort();
        assert_eq!(found, shipped, "a catalog directory is a language");
        assert!(
            found.contains(&String::from("en")),
            "English is the reference"
        );
        for tag in &found {
            let language = tag.parse().expect("a language");
            let dir = root.join(tag);
            let args = placeables(&dir);
            for msg in Msg::all() {
                let words = catalog().try_lookup_with_args(&language, msg.id(), &args);
                assert!(
                    words.is_some_and(|words| !words.trim().is_empty()),
                    "{tag} holds {}",
                    msg.id()
                );
            }
        }
        let english = root.join("en");
        let mut held: Vec<String> = Vec::new();
        for entry in std::fs::read_dir(english).expect("the English catalog") {
            let source =
                std::fs::read_to_string(entry.expect("an entry").path()).expect("a catalog file");
            held.extend(
                source
                    .lines()
                    .filter_map(|line| line.split_once(" ="))
                    .map(|(id, _)| id.to_owned())
                    .filter(|id| {
                        id.starts_with(|c: char| c.is_ascii_lowercase()) && !id.contains(' ')
                    }),
            );
        }
        held.sort();
        let mut declared: Vec<String> = Msg::all()
            .map(|msg| msg.id().to_owned())
            .chain(["number-group", "number-decimal"].map(String::from))
            .collect();
        declared.sort();
        assert_eq!(
            declared, held,
            "every message is declared once and held once"
        );
    }

    #[test]
    fn a_throwaway_catalog_changes_the_words_with_no_code_change() {
        let english = Locale::english();
        assert_eq!(
            "Not found",
            english.lookup(catalog(), Msg::NotFoundTitle.id())
        );
        let throwaway = Locale::parse("qaa").expect("a private-use language");
        assert_eq!(
            "Qot qound",
            throwaway.lookup(&*THROWAWAY, Msg::NotFoundTitle.id())
        );
        assert_eq!(
            "Not found",
            throwaway.lookup(catalog(), Msg::NotFoundTitle.id()),
            "the shipped catalogs fall back to English"
        );
    }

    #[test]
    fn a_number_a_message_takes_is_grouped_in_the_pages_language() {
        let english = Locale::english();
        let words = english.lookup_with(
            catalog(),
            Msg::DashboardRows.id(),
            &[("count", ferrohealth_ui::i18n::Arg::from(312_000_u64))],
        );
        assert!(words.contains("312,000"), "{words}");
    }
}
