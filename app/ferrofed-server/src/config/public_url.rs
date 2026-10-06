// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The gateway's public base URL, `server.public_url`: the absolute URL at
//! which clients, the nodes' authorization servers and the identity services
//! reach `{base}` (§4.1, N28).
//!
//! Three keys repeat it: `auth.audience`, `signing.jwks_uri` and
//! `pmir.callback_url`. With `server.public_url` set, each that is unset
//! takes its value from it, and `signing.jwks_uri` and `pmir.callback_url`,
//! when set, must name the route the gateway serves under it, so a copy that
//! misses the base path is refused at load. No specification governs the
//! key: our own design.

use url::Url;

use crate::base_path::BasePath;
use crate::config::error::Error;

/// The key that holds the public base URL.
pub const KEY: &str = "server.public_url";

/// The public base URL, resolved: an absolute `http` or `https` URL with no
/// user name, password, query or fragment, whose path is `server.base_path`
/// with or without a trailing `/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicUrl {
    written: String,
    parsed: Url,
}

impl PublicUrl {
    /// Resolves `text`, the value of [`KEY`], held to `base`.
    ///
    /// # Errors
    ///
    /// [`Error::Url`] for a value that does not parse, [`Error::PublicUrlForm`]
    /// for one that is no `http` or `https` URL or carries a user name, a
    /// password, a query or a fragment, and [`Error::PublicUrlPath`] for a
    /// path other than `base`.
    pub fn resolve(text: &str, base: &BasePath) -> Result<Self, Error> {
        let key = || KEY.to_owned();
        let parsed = Url::parse(text).map_err(|source| Error::Url { key: key(), source })?;
        if !matches!(parsed.scheme(), "http" | "https")
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(Error::PublicUrlForm);
        }
        let path = match parsed.path().trim_end_matches('/') {
            "" => "/",
            trimmed => trimmed,
        };
        if path != base.as_str() {
            return Err(Error::PublicUrlPath {
                path: path.to_owned(),
                base: base.to_string(),
            });
        }
        Ok(Self {
            written: text.to_owned(),
            parsed,
        })
    }

    /// The URL as the configuration writes it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.written
    }

    /// The absolute URL of `route`, a path that starts with `/`, under
    /// `{base}`.
    ///
    /// # Errors
    ///
    /// [`Error::Url`] naming `key` when the joined text does not parse.
    pub fn route(&self, key: &str, route: &str) -> Result<Url, Error> {
        let base = self.parsed.as_str().trim_end_matches('/');
        Url::parse(&format!("{base}{route}")).map_err(|source| Error::Url {
            key: key.to_owned(),
            source,
        })
    }

    /// Holds `written`, the value of `key`, to the absolute URL of `route`
    /// under `{base}`.
    ///
    /// # Errors
    ///
    /// [`Error::PublicUrlDisagrees`] naming `key` and the URL it should name,
    /// and the errors of [`PublicUrl::route`].
    pub fn agrees(&self, key: &str, written: &Url, route: &str) -> Result<(), Error> {
        let served = self.route(key, route)?;
        if *written == served {
            Ok(())
        } else {
            Err(Error::PublicUrlDisagrees {
                key: key.to_owned(),
                served: served.to_string(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PublicUrl;
    use crate::base_path::BasePath;
    use crate::config::error::Error;

    fn base(text: &str) -> BasePath {
        text.parse().expect("a base path")
    }

    #[test]
    fn the_path_is_the_base_path_with_or_without_a_trailing_slash() {
        for text in ["https://gw.example.org/fed", "https://gw.example.org/fed/"] {
            assert!(PublicUrl::resolve(text, &base("/fed")).is_ok(), "{text}");
        }
        for text in ["https://gw.example.org", "https://gw.example.org/"] {
            assert!(PublicUrl::resolve(text, &base("/")).is_ok(), "{text}");
        }
        assert!(matches!(
            PublicUrl::resolve("https://gw.example.org/", &base("/fed")),
            Err(Error::PublicUrlPath { .. })
        ));
        assert!(matches!(
            PublicUrl::resolve("https://gw.example.org/other", &base("/fed")),
            Err(Error::PublicUrlPath { .. })
        ));
    }

    #[test]
    fn a_url_with_credentials_a_query_a_fragment_or_another_scheme_is_refused() {
        for text in [
            "https://user:secret@gw.example.org/",
            "https://gw.example.org/?a=1",
            "https://gw.example.org/#top",
            "ftp://gw.example.org/",
        ] {
            assert!(
                matches!(
                    PublicUrl::resolve(text, &base("/")),
                    Err(Error::PublicUrlForm)
                ),
                "{text}"
            );
        }
    }

    #[test]
    fn a_route_is_joined_under_the_base_once() {
        let public =
            PublicUrl::resolve("https://gw.example.org/fed/", &base("/fed")).expect("a public URL");
        let route = public
            .route("signing.jwks_uri", "/.well-known/jwks.json")
            .expect("a route");
        assert_eq!(
            "https://gw.example.org/fed/.well-known/jwks.json",
            route.as_str()
        );
    }
}
