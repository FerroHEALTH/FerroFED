// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The suite's callers: the test issuer and its tokens, the key set the
//! server verifies them with, the `[auth]` the server is set up with, and the
//! HTTP Basic users of the scrape.

use std::collections::BTreeSet;
use std::error::Error as StdError;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use ferrofed_server::config::auth::{AuthSettings, IssuerSettings, Verification};
use ferrofed_testkit::issuer::{Claims, Issuer, IssuerError};
use ferrohealth_smart::resource::Settings as KeySetSettings;

use crate::support::toml_path;

/// The issuer identifier of the suite's test issuer.
pub(crate) const ISSUER: &str = "https://issuer.example.test";

/// The audience the gateway under test is known by at the test issuer.
pub(crate) const AUDIENCE: &str = "urn:example:ferrofed-under-test";

/// The suite's test issuer, its key generated once per test process.
#[expect(
    clippy::expect_used,
    reason = "a test process that cannot generate a key pair cannot test anything"
)]
static TEST_ISSUER: LazyLock<Issuer> =
    LazyLock::new(|| Issuer::new(ISSUER).expect("a test key pair should generate"));

/// Returns the suite's test issuer, which every test gateway trusts.
pub(crate) fn issuer() -> &'static Issuer {
    &TEST_ISSUER
}

/// Returns the claims of a token the test gateway admits for every
/// operation: every scope and the purpose of use `TREAT`.
pub(crate) fn claims() -> Claims {
    Claims::new(ISSUER, AUDIENCE)
}

/// Returns a fresh token with the default [`claims`].
pub(crate) fn token() -> Result<String, IssuerError> {
    issuer().mint(&claims())
}

/// A default token minted once per test process: the client credential a
/// test sends explicitly and then searches for at the node, where it must
/// never arrive (§13.1).
#[expect(
    clippy::expect_used,
    reason = "a test process whose issuer cannot sign cannot test anything"
)]
pub(crate) static CLIENT_TOKEN: LazyLock<String> =
    LazyLock::new(|| token().expect("the test issuer should sign"));

/// Returns the `Authorization` value carrying a fresh default [`token`].
pub(crate) fn bearer() -> Result<String, IssuerError> {
    Ok(format!("Bearer {}", token()?))
}

/// Returns the `Authorization` value of another verified caller: the default
/// [`claims`] with `subject` as `sub`.
pub(crate) fn bearer_as(subject: &str) -> Result<String, IssuerError> {
    let mut claims = claims();
    subject.clone_into(&mut claims.sub);
    Ok(format!("Bearer {}", issuer().mint(&claims)?))
}

/// The product scope, `[auth] product_scope`, which admits operators and
/// which no default token carries; the SMART extension form of a string
/// starting with `__` (SMART App Launch 2.2.0, Scopes and Launch Context,
/// Extensibility).
pub(crate) const OPERATOR_SCOPE: &str = "__ferrofed.operator";

/// Returns the `Authorization` value of an operator: the default [`claims`]
/// with [`OPERATOR_SCOPE`] added to the scope.
pub(crate) fn operator_bearer() -> Result<String, IssuerError> {
    bearer_adding_scope(OPERATOR_SCOPE)
}

/// Returns the `Authorization` value of the default [`claims`] with `extra`
/// added to the scope as one more token.
pub(crate) fn bearer_adding_scope(extra: &str) -> Result<String, IssuerError> {
    let mut claims = claims();
    let scope = claims.scope.take().unwrap_or_default();
    claims.scope = Some(format!("{scope} {extra}").trim().to_owned());
    Ok(format!("Bearer {}", issuer().mint(&claims)?))
}

/// The file the suite's test issuer's key set is written to, once per test
/// process, under the target's temporary directory.
#[expect(
    clippy::expect_used,
    reason = "a test process that cannot write its key set file cannot test anything"
)]
static JWKS_FILE: LazyLock<PathBuf> = LazyLock::new(|| {
    let file = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("ferrofed-it-jwks-{}.json", std::process::id()));
    issuer()
        .write_jwks(&file)
        .expect("the key set file should be written");
    file
});

/// Returns the file the suite's test issuer's key set is written to.
pub(crate) fn jwks_file() -> &'static Path {
    &JWKS_FILE
}

/// Returns a file under the target's temporary directory holding
/// `issuer`'s key set as it is now, kept for the life of the test process.
pub(crate) fn jwks_file_of(issuer: &Issuer) -> Result<PathBuf, Box<dyn StdError>> {
    let file = tempfile::NamedTempFile::new_in(env!("CARGO_TARGET_TMPDIR"))?
        .into_temp_path()
        .keep()?;
    issuer.write_jwks(&file)?;
    Ok(file)
}

/// Returns how `issuer`'s tokens addressed to [`AUDIENCE`] are verified:
/// against the key set in `jwks_file`, under `ferrohealth-smart`'s defaults.
pub(crate) fn key_set(issuer: &str, jwks_file: &Path) -> Verification {
    let mut settings = KeySetSettings::new(issuer, AUDIENCE);
    settings.jwks_file = Some(jwks_file.to_owned());
    Verification::KeySet(Box::new(settings))
}

/// Returns the `[auth]` of every test gateway: the test issuer is trusted,
/// its key set read from [`jwks_file`], the default client is a demographic
/// client, and [`OPERATOR_SCOPE`] admits operators.
pub(crate) fn auth() -> AuthSettings {
    let mut trusted = IssuerSettings::new(ISSUER, key_set(ISSUER, jwks_file()));
    trusted.demographic_clients = BTreeSet::from([claims().client_id]);
    AuthSettings {
        audience: vec![AUDIENCE.to_owned()],
        product_scope: Some(OPERATOR_SCOPE.to_owned()),
        issuers: vec![trusted],
        ..AuthSettings::default()
    }
}

/// Returns the `[auth]` and `[trust]` of [`auth`] as configuration text,
/// its issuer an `[[auth.issuer]]` entry, so a test can append another.
/// `role_claims` is empty because beside a second issuer a bare entry,
/// the default included, refuses the start, and the gateway reads no role.
pub(crate) fn auth_toml() -> String {
    format!(
        "\n[auth]\naudience = [\"{AUDIENCE}\"]\nproduct_scope = \"{OPERATOR_SCOPE}\"\nrole_claims = []\n\n[[auth.issuer]]\nissuer = \"{ISSUER}\"\njwks_file = {}\n\n[[trust.issuer]]\nissuer = \"{ISSUER}\"\ndemographic_clients = [\"{}\"]\n",
        toml_path(jwks_file()),
        claims().client_id
    )
}

/// Returns the `[[auth.issuer]]` entry of `issuer`, its key set written to
/// a file as it is now.
pub(crate) fn issuer_toml(issuer: &Issuer) -> Result<String, Box<dyn StdError>> {
    Ok(format!(
        "\n[[auth.issuer]]\nissuer = \"{}\"\njwks_file = {}\n",
        issuer.name(),
        toml_path(&jwks_file_of(issuer)?)
    ))
}

/// The Basic user a scrape of the admin listener authenticates as.
pub(crate) const SCRAPE_USER: &str = "synthetic-scraper";

/// The synthetic password of [`SCRAPE_USER`].
pub(crate) const SCRAPE_PASSWORD: &str = "synthetic-scrape-password";

/// The Argon2id PHC string of [`SCRAPE_PASSWORD`], under a fixed synthetic
/// salt.
pub(crate) const SCRAPE_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c3ludGhldGljLXNhbHQtMTY$4es9c3W3V1fKeaFNam0B7dB/9kjZXP6/+9VB3yhq8ZA";

/// Returns the `Authorization` value of `user` with `password` in the
/// Basic scheme (RFC 7617 §2).
pub(crate) fn basic(user: &str, password: &str) -> String {
    use base64::Engine as _;
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"))
    )
}

/// Returns the `[auth]` of [`auth`] with `users`, each holding
/// [`SCRAPE_PASSWORD`], as its `[[auth.basic.users]]`.
pub(crate) fn auth_with_basic(users: &[&str]) -> Result<AuthSettings, Box<dyn StdError>> {
    use ferrohealth_smart::basic::PasswordStore;
    use ferrohealth_smart::config::{BasicSection, BasicUser};
    use secrecy::SecretString;
    let mut store = PasswordStore::new();
    let mut written = BasicSection::default();
    for user in users {
        store = store.with_user(*user, SecretString::from(SCRAPE_HASH), "")?;
        written.users.push(BasicUser {
            user: (*user).to_owned(),
            password_hash: Some(SecretString::from(SCRAPE_HASH)),
            ..BasicUser::default()
        });
    }
    Ok(AuthSettings {
        basic: Some(ferrofed_server::config::auth::BasicUsers::new(
            store, written,
        )),
        ..auth()
    })
}

/// The `[metrics]` key naming [`SCRAPE_USER`] as the scrape's user, then
/// the `[auth]` that writes that user with HTTP Basic.
pub(crate) fn scraping() -> String {
    format!(
        "scrape_users = [\"{SCRAPE_USER}\"]\n\n[auth]\n\n[[auth.basic.users]]\nuser = \"{SCRAPE_USER}\"\npassword_hash = \"{SCRAPE_HASH}\"\n"
    )
}
