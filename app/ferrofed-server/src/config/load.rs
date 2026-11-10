// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reading the configuration tree with `ferrohealth-config`.
//!
//! The defaults, the TOML file, every `FERROFED__` override and the `--set`
//! pairs, each over the one before. No specification governs the
//! configuration: our own design, in the family's shape.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ferrohealth_config::error::ConfigError;
use ferrohealth_config::loader::{Loaded, Loader};
use ferrohealth_config::provenance::Source;
use ferrohealth_server::cli::ConfigArgs;

use crate::config::error::Error;
use crate::config::revisions::RevisionsFault;
use crate::config::settings::Settings;
use crate::config::{Config, PREFIX};

/// Returns the loader of the gateway's configuration: the `FERROFED`
/// namespace, whose `FERROFED__<SECTION>__<KEY>` overrides set a key and
/// whose `FERROFED_CONFIG` names the file.
///
/// # Errors
/// Returns [`Error::Load`] when the namespace is refused, which it never is
/// for [`PREFIX`].
pub fn loader() -> Result<Loader, Error> {
    let fault = |source| Error::Load {
        file: None,
        source: Box::new(source),
    };
    endpoint_defaults(
        Loader::new(PREFIX)
            .map_err(fault)?
            .defaults("server", &crate::config::server::defaults())
            .map_err(fault)?,
    )
    .map_err(fault)
}

/// `loader` with each outbound table's own `endpoint` defaults, layered
/// under a table, or each element of an array of tables, a source writes, so
/// a key the operator leaves out keeps the table's default and an unwritten
/// table stays absent (FerroHEALTH/FerroSYS#353).
fn endpoint_defaults(loader: Loader) -> Result<Loader, ConfigError> {
    let loader = loader
        .defaults_if_written(
            "audit.repository",
            "endpoint",
            &crate::config::audit::FeedRepository::default().endpoint,
        )?
        .defaults_if_written(
            "eehrxf.terminology",
            "endpoint",
            &crate::config::eehrxf::TerminologyServer::default().endpoint,
        )?;
    #[cfg(feature = "binding-ihe")]
    let loader = loader
        .defaults_each(
            "pixm.manager",
            "endpoint",
            &crate::config::ihe::pixm::PixManager::default().endpoint,
        )?
        .defaults_if_written(
            "pdqm",
            "endpoint",
            &crate::config::ihe::pdqm::Pdqm::default().endpoint,
        )?
        .defaults_if_written(
            "pmir",
            "endpoint",
            &crate::config::ihe::pmir::Pmir::default().endpoint,
        )?
        .defaults_if_written(
            "registry.mcsd",
            "endpoint",
            &crate::config::ihe::mcsd::McsdDirectory::default().endpoint,
        )?
        .defaults_each(
            "xcpd.gateway",
            "endpoint",
            &crate::config::ihe::xcpd::XcpdGateway::default().endpoint,
        )?;
    #[cfg(feature = "binding-nl")]
    let loader = loader
        .defaults_if_written(
            "nl_gf.nvi",
            "endpoint",
            &crate::config::nl::nvi::Nvi::default().endpoint,
        )?
        .defaults_if_written(
            "nl_gf.mitz",
            "endpoint",
            &crate::config::nl::mitz::Mitz::default().endpoint,
        )?;
    Ok(loader)
}

/// The dotted key whose source decides whether the drain follows the
/// request timeout ([`Config::shutdown_timeout_written`]).
const SHUTDOWN_TIMEOUT: &str = "server.shutdown_timeout";

impl Config {
    /// Reads the configuration of the running process: the file `sources`
    /// names (or `FERROFED_CONFIG` does), the environment, and the `--set`
    /// pairs of `sources`, last.
    ///
    /// The [`Loaded`] keeps the merged table the configuration revisions
    /// record and `ferrofed report` writes, redacted, and where each key
    /// came from.
    ///
    /// # Errors
    /// Returns [`Error::Load`] with every refusal of the loader, naming the
    /// file a refusal of its content sits in, and
    /// [`RevisionsFault::ConsoleKeyInFile`] when
    /// a source sets a resource the operator console owns.
    pub fn load(sources: &ConfigArgs) -> Result<Loaded<Self>, Error> {
        let loader = loader()?;
        let file = sources
            .config
            .clone()
            .or_else(|| std::env::var_os(loader.config_variable()).map(PathBuf::from));
        loader
            .load_with::<Self>(sources.config.as_deref(), &sources.sets)
            .map_err(|source| refused(file.as_deref(), source))
            .and_then(checked)
            .and_then(console_free)
    }

    /// Reads `text` as the configuration file and applies `environment`
    /// over it, as [`load`](Self::load) does with no `--set` pair.
    ///
    /// Reading the sources and reading the process environment are separate
    /// so a test can state both: setting an environment variable is `unsafe`
    /// in edition 2024 and this workspace forbids `unsafe`.
    ///
    /// # Errors
    /// Every error [`load`](Self::load) returns but a file that cannot be
    /// read and a resource of the operator console, which a revision's
    /// table, read through here, holds.
    pub fn from_sources(
        text: Option<&str>,
        environment: &BTreeMap<String, String>,
    ) -> Result<Self, Error> {
        Self::loaded_from_sources(text, environment).map(|loaded| {
            let written = shutdown_written(&loaded);
            let mut config = loaded.into_parts().0;
            config.shutdown_timeout_written = written;
            config
        })
    }

    /// Reads `text` and `environment` as [`from_sources`](Self::from_sources)
    /// does, and keeps the merged table and the provenance.
    ///
    /// # Errors
    /// Every error [`from_sources`](Self::from_sources) returns.
    pub fn loaded_from_sources(
        text: Option<&str>,
        environment: &BTreeMap<String, String>,
    ) -> Result<Loaded<Self>, Error> {
        loader()?
            .load_sources::<Self>(text, None, environment)
            .map_err(|source| refused(None, source))
            .and_then(checked)
    }

    /// Resolves the configuration `loaded` holds, knowing which of its
    /// sources set `server.shutdown_timeout`.
    ///
    /// # Errors
    /// Every error [`Config::resolve`] returns.
    pub fn resolve_loaded(loaded: &Loaded<Self>) -> Result<Settings, Error> {
        let mut config = loaded.settings().clone();
        config.shutdown_timeout_written = shutdown_written(loaded);
        config.resolve()
    }
}

/// Whether a source of `loaded` set `server.shutdown_timeout`.
fn shutdown_written(loaded: &Loaded<Config>) -> bool {
    loaded.provenance().source(SHUTDOWN_TIMEOUT) != Source::Default
}

/// Reads `text`, a table the gateway stores such as a revision's, as TOML,
/// refused as the loader refuses a file that is not TOML: by its position,
/// quoting no value.
///
/// # Errors
/// Returns [`Error::Load`] for text that is not TOML.
pub(crate) fn table(text: &str) -> Result<toml::Table, Error> {
    // NOTE: no specification governs this: our own design; the table is read as
    // written, so the loader that reads it layers no default and renames no key.
    Loader::new(PREFIX)
        .map_err(|source| Error::Load {
            file: None,
            source: Box::new(source),
        })?
        .load_sources::<toml::Table>(Some(text), None, &BTreeMap::new())
        .map(|loaded| loaded.into_parts().0)
        .map_err(|source| refused(None, source))
}

/// Sets each dotted key of `pairs` in `table`, typed as the configuration
/// tree takes it: a number, a boolean or an array where the tree wants one,
/// and text elsewhere, as a `--set` pair is.
///
/// # Errors
/// Returns [`Error::Load`] for a pair that names no key of the tree, or
/// whose value the tree refuses.
pub(crate) fn with_pairs(
    table: &toml::Table,
    pairs: &[(String, String)],
) -> Result<toml::Table, Error> {
    let text = toml::to_string(table).map_err(Error::assemble)?;
    loader()?
        .load_sources_with::<Config>(Some(&text), None, &BTreeMap::new(), pairs)
        .map(|loaded| loaded.table_with_secrets().clone())
        .map_err(|source| refused(None, source))
}

/// The refusal of a load whose sources `ferrohealth-config` refused, naming
/// `file` unless the refusal is that the file could not be read, which
/// names it itself.
fn refused(file: Option<&Path>, source: ConfigError) -> Error {
    let file = match source {
        ConfigError::Read { .. } => None,
        _ => file.map(Path::to_path_buf),
    };
    Error::Load {
        file,
        source: Box::new(source),
    }
}

/// Refuses every key a release retired that the sources set
/// (`config::retired`).
fn checked(loaded: Loaded<Config>) -> Result<Loaded<Config>, Error> {
    crate::config::retired::check(loaded.settings())?;
    Ok(loaded)
}

/// Refuses a file or an override that sets a resource the operator console
/// owns, which only a revision holds.
fn console_free(loaded: Loaded<Config>) -> Result<Loaded<Config>, Error> {
    // NOTE: no specification governs this: our own design; what the console owns
    // lives only in a revision, so the file never sets it and nothing is written back.
    match crate::config::tree::console_keys(loaded.table_with_secrets())
        .into_iter()
        .next()
    {
        Some(key) => Err(RevisionsFault::ConsoleKeyInFile { key }.into()),
        None => Ok(loaded),
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use std::collections::BTreeMap;

    #[test]
    fn an_absent_file_and_an_empty_environment_leave_the_defaults() {
        let config = Config::from_sources(None, &BTreeMap::new()).expect("the defaults parse");
        assert_eq!(Config::default(), config);
        assert_eq!("127.0.0.1:8080", config.server.listen.to_string());
        assert_eq!(1024 * 1024, config.server.body_limit.bytes());
        assert!(config.server.drain_delay.is_zero());
        assert!(!config.shutdown_timeout_written);
        assert_eq!(
            Some(512),
            config
                .server
                .max_concurrent_requests
                .map(std::num::NonZeroUsize::get),
            "the gateway's own default"
        );
        assert!(config.credentials.is_empty());
        let settings = config.resolve().expect("the defaults resolve");
        assert_eq!(
            settings.server.request_timeout, settings.server.shutdown_timeout,
            "the drain defaults to the request timeout"
        );
    }

    #[test]
    fn a_pair_is_set_through_its_parents_typed_as_the_tree_takes_it() {
        let table = super::with_pairs(
            &toml::Table::new(),
            &[("federation.consent.disclose".to_owned(), "false".to_owned())],
        )
        .expect("the pair names a key of the tree");
        assert_eq!(
            Some(&toml::Value::Boolean(false)),
            crate::running::at(&table, "federation.consent.disclose")
        );
    }

    #[test]
    fn a_pair_under_a_value_is_refused() {
        let flat: toml::Table = toml::from_str(
            "[federation]
best_effort = true
",
        )
        .expect("the table reads");
        let refused = super::with_pairs(
            &flat,
            &[("federation.best_effort.x".to_owned(), "1".to_owned())],
        );
        assert!(refused.is_err(), "{refused:?}");
    }

    /// The `endpoint` table `text` sets under `[pdqm]` and each
    /// `[[pixm.manager]]`, as loaded.
    fn tables(text: &str) -> Config {
        Config::from_sources(Some(text), &BTreeMap::new()).expect("the configuration loads")
    }

    #[cfg(feature = "binding-ihe")]
    #[test]
    fn a_partly_written_endpoint_table_keeps_its_tables_defaults() {
        let unwritten = tables("");
        assert!(unwritten.pdqm.is_none(), "an unwritten [pdqm] stays absent");
        assert!(unwritten.pixm.is_none(), "an unwritten [pixm] stays absent");
        let written = tables(
            "[pdqm]\nmaster = \"urn:oid:2.999.1\"\n\n[pdqm.endpoint]\nurl = \"https://pdq.example.org/fhir/\"\n\n\
             [[pixm.manager]]\nendpoint.url = \"https://a.example.org/fhir/\"\n\n\
             [[pixm.manager]]\nendpoint.url = \"https://b.example.org/fhir/\"\nendpoint.retry.max_attempts = 3\n",
        );
        let pdqm = written.pdqm.expect("[pdqm] is written");
        assert_eq!(1000, pdqm.endpoint.timeout.as_millis(), "PDQm keeps its 1s");
        assert_eq!(
            1, pdqm.endpoint.retry.max_attempts,
            "PDQm tries a call once"
        );
        let managers = written.pixm.expect("[pixm] is written").manager;
        let attempts: Vec<u32> = managers
            .iter()
            .map(|manager| manager.endpoint.retry.max_attempts)
            .collect();
        assert_eq!(
            vec![1, 3],
            attempts,
            "each element keeps the default it does not set"
        );
    }

    #[test]
    fn a_partly_written_audit_repository_keeps_its_delivery_defaults() {
        let written = tables(
            "[audit.repository]\nhostname = \"gateway.example.org\"\n\n\
             [audit.repository.endpoint]\nurl = \"https://arr.example.org/fhir\"\n",
        );
        let endpoint = written
            .audit
            .repository
            .expect("[audit.repository] is written")
            .endpoint;
        assert_eq!(5000, endpoint.timeout.as_millis());
        assert_eq!(60_000, endpoint.retry.max_backoff.as_millis());
    }
}
