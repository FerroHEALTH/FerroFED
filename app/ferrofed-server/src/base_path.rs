// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The path of the deployment's base URL, `{base}`, which every route the
//! gateway serves sits under (§4.1, N28).
//!
//! The specification mandates and reserves no prefix: the deployment chooses
//! its base URL, `/rest/openehr` included only when the deployment chooses it
//! (N28, CP-21). The gateway serves `{base}/`, `{base}/health` and
//! `{base}/v1/…`, and with the default base, `/`, it serves them at the root.

use std::fmt;
use std::str::FromStr;

/// The path the gateway is mounted at: `/`, or one or more `/`-separated
/// segments with no trailing `/`.
///
/// # Examples
///
/// ```
/// use ferrofed_server::base_path::BasePath;
///
/// let base: BasePath = "/fed/openehr".parse()?;
/// assert_eq!(base.join("/v1/query/aql"), "/fed/openehr/v1/query/aql");
/// assert_eq!(BasePath::default().join("/v1/query/aql"), "/v1/query/aql");
/// assert!("/fed/".parse::<BasePath>().is_err());
/// # Ok::<(), ferrofed_server::base_path::BasePathError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasePath(String);

impl BasePath {
    /// The root, `/`.
    const ROOT: &'static str = "/";

    /// Whether this is the root, `/`.
    #[must_use]
    pub fn is_root(&self) -> bool {
        self.0 == Self::ROOT
    }

    /// The path as configured: `/`, or `/seg(/seg)*`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The path `path`, which starts with `/`, takes under this base.
    #[must_use]
    pub fn join(&self, path: &str) -> String {
        if self.is_root() {
            path.to_owned()
        } else {
            format!("{}{path}", self.0)
        }
    }
}

impl Default for BasePath {
    fn default() -> Self {
        Self(Self::ROOT.to_owned())
    }
}

impl fmt::Display for BasePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for BasePath {
    type Err = BasePathError;

    /// Reads a base path, refusing what no request path can be under.
    ///
    /// A base path starts with `/`, has no trailing `/` unless it is `/`,
    /// carries no query or fragment, and has no empty, `.` or `..` segment.
    /// Each segment is the RFC 3986 `segment` an `http` request target admits,
    /// so the path is written as it is sent.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text == Self::ROOT {
            return Ok(Self::default());
        }
        let Some(rest) = text.strip_prefix('/') else {
            return Err(BasePathError::NotAbsolute);
        };
        if text.contains(['?', '#']) {
            return Err(BasePathError::QueryOrFragment);
        }
        if text.ends_with('/') {
            return Err(BasePathError::TrailingSlash);
        }
        if rest
            .split('/')
            .any(|segment| matches!(segment, "" | "." | ".."))
        {
            return Err(BasePathError::Segment);
        }
        let parsed = text
            .parse::<http::uri::PathAndQuery>()
            .map_err(BasePathError::NotAPath)?;
        if parsed.as_str() != text {
            return Err(BasePathError::Segment);
        }
        Ok(Self(text.to_owned()))
    }
}

/// Why a configured base path is refused.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BasePathError {
    /// The path does not start with `/`.
    #[error("a base path starts with `/`")]
    NotAbsolute,
    /// The path ends with `/` and is not `/`.
    #[error("a base path other than `/` has no trailing `/`")]
    TrailingSlash,
    /// The path carries a `?` or a `#`.
    #[error("a base path carries no query and no fragment")]
    QueryOrFragment,
    /// A segment is empty, `.` or `..`, or not written as a request sends it.
    #[error("a base path has no empty, `.` or `..` segment, and is written as a request sends it")]
    Segment,
    /// The path is not a request path.
    #[error("a base path is a request path")]
    NotAPath(#[source] http::uri::InvalidUri),
}

#[cfg(test)]
mod tests {
    use super::{BasePath, BasePathError};

    #[test]
    fn the_root_and_a_prefix_are_accepted() {
        assert!("/".parse::<BasePath>().unwrap().is_root());
        let base = "/fed/openehr".parse::<BasePath>().unwrap();
        assert_eq!("/fed/openehr", base.as_str());
        assert_eq!("/fed/openehr/health", base.join("/health"));
        assert_eq!(
            "/rest/openehr",
            "/rest/openehr".parse::<BasePath>().unwrap().as_str(),
            "N28 reserves no prefix, and forbids none the deployment chooses"
        );
    }

    #[test]
    fn every_malformed_base_is_refused() {
        let refused = [
            ("", "NotAbsolute"),
            ("fed", "NotAbsolute"),
            ("/fed/", "TrailingSlash"),
            ("/fed?x=1", "QueryOrFragment"),
            ("/fed#top", "QueryOrFragment"),
            ("//fed", "Segment"),
            ("/fed//openehr", "Segment"),
            ("/fed/./openehr", "Segment"),
            ("/fed/..", "Segment"),
            ("/fe d", "NotAPath"),
        ];
        for (text, kind) in refused {
            let error = text.parse::<BasePath>().unwrap_err();
            let named = match error {
                BasePathError::NotAbsolute => "NotAbsolute",
                BasePathError::TrailingSlash => "TrailingSlash",
                BasePathError::QueryOrFragment => "QueryOrFragment",
                BasePathError::Segment => "Segment",
                BasePathError::NotAPath(_) => "NotAPath",
            };
            assert_eq!(kind, named, "{text:?}");
        }
    }
}
