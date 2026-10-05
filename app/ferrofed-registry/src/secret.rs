// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The two types every credential the gateway is configured with is held in.
//!
//! Redaction is a property of the type. A [`Secret`] (a bearer token, a
//! password) renders as [`REDACTED`] through `Debug`, `Display` and
//! `Serialize`; a [`SecretUrl`] (a URL that may carry a user name and
//! password in its userinfo, RFC 3986 §3.2.1, or a credential in its query)
//! renders with those parts replaced by [`REDACTED`]. Each holds its value in
//! a [`SecretString`], zeroed on drop, deserializes from a plain string, and
//! is reached only through `expose`. They live in the registry crate because
//! the registry document, the identity bindings and the server configuration
//! all hold one. No specification governs this: our own design.

use std::fmt;

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The fixed text every rendering of a secret shows in its place.
pub const REDACTED: &str = "***";

/// A secret string that never renders itself.
///
/// The value is a [`SecretString`], zeroed on drop. `Debug`, `Display` and
/// `Serialize` all show [`REDACTED`].
///
/// # Examples
///
/// ```
/// use ferrofed_registry::secret::Secret;
///
/// let token = Secret::new("Qz7-token");
/// assert_eq!(format!("{token:?}"), "\"***\"");
/// assert_eq!(token.to_string(), "***");
/// assert_eq!(token.expose(), "Qz7-token");
/// ```
#[derive(Clone)]
pub struct Secret(SecretString);

impl Secret {
    /// Creates a secret holding `value`.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(SecretString::from(value.into()))
    }

    /// Returns the value, for the one place a request is composed from it.
    ///
    /// Never log or format the result.
    #[must_use]
    pub fn expose(&self) -> &str {
        self.0.expose_secret()
    }

    /// Returns the value still wrapped, for a client that takes a
    /// [`SecretString`] and composes its request from that.
    #[must_use]
    pub fn to_secret_string(&self) -> SecretString {
        self.0.clone()
    }

    /// Whether the secret is the empty string.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.expose_secret().is_empty()
    }
}

impl From<SecretString> for Secret {
    fn from(value: SecretString) -> Self {
        Self(value)
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(REDACTED, f)
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

impl PartialEq for Secret {
    fn eq(&self, other: &Self) -> bool {
        self.0.expose_secret() == other.0.expose_secret()
    }
}

impl Eq for Secret {}

impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self::new)
    }
}

impl Serialize for Secret {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(REDACTED)
    }
}

/// A URL or connection string kept as written, whose renderings never show
/// a credential.
///
/// [`SecretUrl::expose`] returns the text verbatim for connecting with it.
/// `Debug`, `Display` and `Serialize` show [`SecretUrl::redacted`].
///
/// # Examples
///
/// ```
/// use ferrofed_registry::secret::SecretUrl;
///
/// let url = SecretUrl::new("postgres://ferrofed:Qz7pw@db.example.org:5432/ferrofed");
/// assert_eq!(url.to_string(), "postgres://***@db.example.org:5432/ferrofed");
/// assert_eq!(url.expose(), "postgres://ferrofed:Qz7pw@db.example.org:5432/ferrofed");
/// ```
#[derive(Clone, Default)]
pub struct SecretUrl(SecretString);

impl SecretUrl {
    /// Creates a URL holding `url` verbatim.
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        Self(SecretString::from(url.into()))
    }

    /// Returns the URL as written, credentials included, for connecting with
    /// it.
    ///
    /// Never log or format the result.
    #[must_use]
    pub fn expose(&self) -> &str {
        self.0.expose_secret()
    }

    /// Whether the URL is the empty string.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.expose_secret().is_empty()
    }

    /// Returns the URL with every part that may carry a credential replaced
    /// by [`REDACTED`].
    ///
    /// The userinfo becomes `***@` (`scheme://***@host/path`) and the query
    /// becomes `?***`; the scheme, the host, the port, the path and the
    /// fragment show as written. Text with no `://` (a libpq key/value
    /// connection string, which carries its password outside any userinfo)
    /// shows as [`REDACTED`] whole.
    #[must_use]
    pub fn redacted(&self) -> String {
        redact(self.0.expose_secret())
    }
}

/// Returns `url` with its userinfo and its query replaced by [`REDACTED`].
///
/// The userinfo is everything after `://` up to an `@`, since a host holds no
/// `@` (RFC 3986 §3.2). When the text parses as a URL (the WHATWG URL
/// Standard, as `url` and every client built on it read it), the authority
/// ends at its first `/`, `?` or `#`, so an `@` after it is in the path
/// or the query and shows as written. When it does not parse, the userinfo
/// runs to its last `@`, so a password holding a `/`, a `?` or a `#` is still
/// found. The text is never decoded, so it is redacted as written.
fn redact(url: &str) -> String {
    if url.is_empty() {
        return String::new();
    }
    let Some((scheme, rest)) = url.split_once("://") else {
        return REDACTED.to_owned();
    };
    let at = if url::Url::parse(url).is_ok() {
        let authority = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        rest.get(..authority).and_then(|text| text.rfind('@'))
    } else {
        rest.rfind('@')
    };
    let (userinfo, rest) = match at {
        Some(at) => (
            format!("{REDACTED}@"),
            rest.get(at + 1..).unwrap_or_default(),
        ),
        None => (String::new(), rest),
    };
    let end = rest.find(['?', '#']).unwrap_or(rest.len());
    let (hierarchy, tail) = rest.split_at_checked(end).unwrap_or((rest, ""));
    let tail = match tail.strip_prefix('?') {
        Some(query) => {
            let fragment = query
                .find('#')
                .and_then(|at| query.get(at..))
                .unwrap_or_default();
            format!("?{REDACTED}{fragment}")
        }
        None => tail.to_owned(),
    };
    format!("{scheme}://{userinfo}{hierarchy}{tail}")
}

impl From<SecretString> for SecretUrl {
    fn from(url: SecretString) -> Self {
        Self(url)
    }
}

impl fmt::Debug for SecretUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.redacted(), f)
    }
}

impl fmt::Display for SecretUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.redacted())
    }
}

impl PartialEq for SecretUrl {
    fn eq(&self, other: &Self) -> bool {
        self.0.expose_secret() == other.0.expose_secret()
    }
}

impl Eq for SecretUrl {}

impl<'de> Deserialize<'de> for SecretUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self::new)
    }
}

impl Serialize for SecretUrl {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.redacted())
    }
}

#[cfg(test)]
mod tests {
    use super::{REDACTED, Secret, SecretUrl};

    const SENTINEL: &str = "Qz7sentinel";

    #[test]
    fn a_secret_renders_as_the_placeholder_everywhere() {
        let secret = Secret::new(SENTINEL);
        for shown in [
            format!("{secret:?}"),
            format!("{secret:#?}"),
            format!("{secret}"),
            toml::Value::try_from(&secret)
                .expect("a secret serializes")
                .to_string(),
        ] {
            assert!(!shown.contains(SENTINEL), "{shown}");
            assert!(shown.contains(REDACTED), "{shown}");
        }
        assert_eq!(SENTINEL, secret.expose());
    }

    #[test]
    fn a_secret_reads_from_a_plain_string() {
        let table: toml::Table = toml::from_str("token = \"Qz7sentinel\"").expect("toml");
        let secret: Secret = table
            .get("token")
            .cloned()
            .expect("the key")
            .try_into()
            .expect("a string reads as a secret");
        assert_eq!(SENTINEL, secret.expose());
        assert!(!secret.is_empty());
        assert!(Secret::new("").is_empty());
    }

    #[test]
    fn secrets_compare_by_value() {
        assert_eq!(Secret::new(SENTINEL), Secret::new(SENTINEL));
        assert_ne!(Secret::new(SENTINEL), Secret::new("other"));
        assert_eq!(SecretUrl::new(SENTINEL), SecretUrl::new(SENTINEL));
        assert_ne!(SecretUrl::new(SENTINEL), SecretUrl::new("other"));
    }

    #[test]
    fn a_url_with_userinfo_renders_without_it() {
        let url = SecretUrl::new("postgres://ferrofed:Qz7example@db.example.org:5432/ferrofed");
        let redacted = "postgres://***@db.example.org:5432/ferrofed";
        assert_eq!(redacted, url.redacted());
        assert_eq!(redacted, url.to_string());
        assert_eq!(format!("{redacted:?}"), format!("{url:?}"));
        assert_eq!(
            toml::Value::String(redacted.to_owned()),
            toml::Value::try_from(&url).expect("a URL serializes")
        );
        assert_eq!(
            "postgres://ferrofed:Qz7example@db.example.org:5432/ferrofed",
            url.expose()
        );
    }

    #[test]
    fn every_part_that_may_carry_a_credential_is_redacted() {
        for (raw, redacted) in [
            (
                "https://Qz7user@pix.example.org/fhir",
                "https://***@pix.example.org/fhir",
            ),
            (
                "https://u:p@ss@pix.example.org/",
                "https://***@pix.example.org/",
            ),
            (
                "https://u:p@pix.example.org?x=1",
                "https://***@pix.example.org?***",
            ),
            (
                "postgres://db.example.org/ferrofed?password=Qz7sentinel#f",
                "postgres://db.example.org/ferrofed?***#f",
            ),
            ("host=db.example.org password=Qz7sentinel", "***"),
            ("u:Qz7sentinel@db.example.org/x", "***"),
        ] {
            assert_eq!(redacted, SecretUrl::new(raw).redacted(), "{raw}");
        }
    }

    #[test]
    fn a_password_holding_a_delimiter_is_redacted() {
        for (raw, redacted) in [
            ("https://u:p/x@host", "https://***@host"),
            (
                "postgres://ferrofed:Qz7/sentinel@db.example.org:5432/ferrofed",
                "postgres://***@db.example.org:5432/ferrofed",
            ),
            (
                "postgres://ferrofed:Qz7?example@db.example.org/ferrofed?sslmode=require",
                "postgres://***@db.example.org/ferrofed?***",
            ),
            (
                "https://u:Qz7#sentinel@cdr-a.example.org/openehr",
                "https://***@cdr-a.example.org/openehr",
            ),
        ] {
            assert_eq!(redacted, SecretUrl::new(raw).redacted(), "{raw}");
        }
    }

    #[test]
    fn an_at_sign_after_the_authority_is_not_userinfo() {
        for (raw, redacted) in [
            (
                "https://cdr-a.example.org/path/@handle",
                "https://cdr-a.example.org/path/@handle",
            ),
            (
                "https://cdr-a.example.org/openehr?owner=someone@example.org",
                "https://cdr-a.example.org/openehr?***",
            ),
            (
                "https://cdr-a.example.org/openehr#section@b",
                "https://cdr-a.example.org/openehr#section@b",
            ),
        ] {
            assert_eq!(redacted, SecretUrl::new(raw).redacted(), "{raw}");
        }
    }

    #[test]
    fn text_that_does_not_parse_is_redacted_to_its_last_at_sign() {
        assert_eq!(
            "https://***@handle",
            SecretUrl::new("https://cdr a.example.org/path/@handle").redacted()
        );
    }

    #[test]
    fn a_url_without_a_credential_renders_as_written() {
        for raw in [
            "https://cdr-a.example.org/openehr",
            "https://cdr-a.example.org/path/@handle",
            "http://127.0.0.1:4317/",
            "",
        ] {
            assert_eq!(raw, SecretUrl::new(raw).redacted(), "{raw}");
        }
        assert!(SecretUrl::default().is_empty());
    }
}
