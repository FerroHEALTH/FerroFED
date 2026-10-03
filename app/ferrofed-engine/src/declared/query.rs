// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The parameter names of a client query string, held to those its ITS-REST
//! operation declares (§5.4.1, N33).
//!
//! Only names are read here, percent-decoded, to find the first one a rule
//! does not admit; a parameter's value is decoded by the generated `*Params`
//! of `openehr-its`, or forwarded as received.

use openehr_its::rest::routes::RouteMatch;

use crate::hygiene::{self, UnlistedParameter};

/// Checks that `operation` declares every parameter of `query`, a query
/// string the gateway consumes and never forwards.
///
/// A parameter's name is compared percent-decoded, and an empty pair
/// (`a=1&&b=2`) carries nothing and is ignored.
///
/// # Errors
///
/// Returns [`UnlistedParameter`] naming the first other parameter by its
/// position, never by its name or value, either of which may be the
/// identifier.
pub fn every_declared(operation: &RouteMatch, query: &str) -> Result<(), UnlistedParameter> {
    admitted(query, |name| operation.query_key(name).is_some())
}

/// Checks that `admits` accepts the percent-decoded name of every parameter
/// of `query`, an empty pair ignored.
pub(crate) fn admitted(
    query: &str,
    admits: impl Fn(&str) -> bool,
) -> Result<(), UnlistedParameter> {
    let unlisted = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .position(|pair| {
            let name = pair.split('=').next().unwrap_or(pair);
            !admits(&hygiene::percent_decoded(name))
        });
    match unlisted {
        Some(index) => Err(UnlistedParameter {
            position: index.saturating_add(1),
        }),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::every_declared;
    use crate::hygiene::UnlistedParameter;
    use http::Method;
    use openehr_its::rest::routes::{Lookup, RouteMatch, lookup};

    fn by_subject() -> RouteMatch {
        match lookup(&Method::GET, "/ehr") {
            Lookup::Matched(matched) => matched,
            other => panic!("GET /ehr names no operation: {other:?}"),
        }
    }

    #[test]
    fn a_consumed_query_admits_the_subject_parameters_and_nothing_undeclared() {
        let operation = by_subject();
        let declared = "subject_id=4711&&subject%5Fnamespace=x";
        assert_eq!(Ok(()), every_declared(&operation, declared));
        assert_eq!(Ok(()), every_declared(&operation, ""));
        let refused = every_declared(&operation, "subject_id=4711&patient=O%27Sentinel-4711");
        assert_eq!(Err(UnlistedParameter { position: 2 }), refused);
    }
}
