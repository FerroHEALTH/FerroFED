// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The two types every credential the gateway is configured with is held in.
//!
//! Redaction is a property of the type. A [`Secret`] (a bearer token, a
//! password) renders as [`REDACTED`] through `Debug`, `Display` and
//! `Serialize`; a [`SecretUrl`] (a URL that may carry a user name and
//! password in its userinfo, RFC 3986 §3.2.1) renders with that userinfo
//! replaced by [`REDACTED`]. Each deserializes from a plain string, and the
//! value is reached only through `expose`. They live in the registry crate
//! because the registry document, the identity bindings and the server
//! configuration all hold one. No specification governs this: our own design.

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

/// A URL kept as written, whose renderings never show its userinfo.
///
/// [`SecretUrl::expose`] returns the URL verbatim for connecting with it.
/// `Debug`, `Display` and `Serialize` show [`SecretUrl::redacted`], which
/// replaces the userinfo with [`REDACTED`].
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
#[derive(Clone, Default, PartialEq, Eq)]
pub struct SecretUrl(String);

impl SecretUrl {
    /// Creates a URL holding `url` verbatim.
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        Self(url.into())
    }

    /// Returns the URL as written, userinfo included, for connecting with it.
    ///
    /// Never log or format the result.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Whether the URL is the empty string.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the URL with its userinfo replaced by [`REDACTED`]
    /// (`scheme://***@host/path`); a URL without userinfo is returned as
    /// written.
    #[must_use]
    pub fn redacted(&self) -> String {
        redact_userinfo(&self.0)
    }
}

/// Returns `url` with the userinfo of its authority replaced by [`REDACTED`].
///
/// The authority runs from after `://` (or from the start, when there is
/// none) to the first `/`, `?` or `#`, and the userinfo is everything in it
/// before its last `@`, since a host holds no `@` (RFC 3986 §3.2). The text
/// is never decoded, so it is redacted as written.
fn redact_userinfo(url: &str) -> String {
    let (scheme, rest) = match url.split_once("://") {
        Some((scheme, rest)) => (Some(scheme), rest),
        None => (None, url),
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = rest.get(..end).unwrap_or(rest);
    let Some(at) = authority.rfind('@') else {
        return url.to_owned();
    };
    let after = rest.get(at + 1..).unwrap_or_default();
    match scheme {
        Some(scheme) => format!("{scheme}://{REDACTED}@{after}"),
        None => format!("{REDACTED}@{after}"),
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
    }

    #[test]
    fn a_url_with_userinfo_renders_without_it() {
        let url = SecretUrl::new("postgres://ferrofed:Qz7sentinel@db.example.org:5432/ferrofed");
        let redacted = "postgres://***@db.example.org:5432/ferrofed";
        assert_eq!(redacted, url.redacted());
        assert_eq!(redacted, url.to_string());
        assert_eq!(format!("{redacted:?}"), format!("{url:?}"));
        assert_eq!(
            toml::Value::String(redacted.to_owned()),
            toml::Value::try_from(&url).expect("a URL serializes")
        );
        assert_eq!(
            "postgres://ferrofed:Qz7sentinel@db.example.org:5432/ferrofed",
            url.expose()
        );
    }

    #[test]
    fn userinfo_is_redacted_in_every_form_of_authority() {
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
                "https://***@pix.example.org?x=1",
            ),
            ("u:Qz7sentinel@db.example.org/x", "***@db.example.org/x"),
        ] {
            assert_eq!(redacted, SecretUrl::new(raw).redacted(), "{raw}");
        }
    }

    #[test]
    fn a_url_without_userinfo_renders_as_written() {
        for raw in [
            "https://cdr-a.example.org/openehr",
            "https://cdr-a.example.org/path/@handle",
            "https://cdr-a.example.org?who=@me",
            "",
        ] {
            assert_eq!(raw, SecretUrl::new(raw).redacted(), "{raw}");
        }
        assert!(SecretUrl::default().is_empty());
    }
}
