// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credentials of one endpoint, and every other secret, with every
//! `_file` sibling read at boot.

use std::path::Path;

use openehr_its::rest::client::{BasicPart, InvalidCredentials};
use secrecy::SecretString;

use crate::config::Credentials;
use crate::config::error::{BasicFault, Error};
use crate::config::settings::Scheme;

/// Returns the scheme `credentials` describes, once it is known to fit the
/// `Authorization` header it is sent in.
pub(super) fn resolve_credentials(
    section: &str,
    credentials: &Credentials,
) -> Result<Scheme, Error> {
    let token = secret(
        &format!("{section}.bearer_token"),
        credentials.bearer_token.as_deref(),
        credentials.bearer_token_file.as_deref(),
    )?;
    let password = secret(
        &format!("{section}.password"),
        credentials.password.as_deref(),
        credentials.password_file.as_deref(),
    )?;
    match (token, credentials.user.as_deref(), password) {
        (Some(_), Some(_), _) | (Some(_), None, Some(_)) => Err(Error::Scheme {
            section: section.to_owned(),
        }),
        (Some(token), None, None) => {
            let key = source_key(
                section,
                "bearer_token",
                credentials.bearer_token_file.is_some(),
            );
            openehr_its::rest::client::Credentials::bearer(token.clone())
                .header_value()
                .map_err(|source| Error::Authorization { key, source })?;
            Ok(Scheme::Bearer(token))
        }
        (None, Some(user), Some(password)) => {
            openehr_its::rest::client::Credentials::basic(user, password.clone())
                .header_value()
                .map_err(|source| {
                    basic_refusal(section, credentials.password_file.is_some(), source)
                })?;
            Ok(Scheme::Basic {
                user: user.to_owned(),
                password,
            })
        }
        (None, Some(_), None) => Err(Error::Missing {
            key: format!("{section}.password"),
        }),
        (None, None, Some(_)) => Err(Error::Missing {
            key: format!("{section}.user"),
        }),
        (None, None, None) => Err(Error::NoScheme {
            section: section.to_owned(),
        }),
    }
}

/// The key the secret `name` of `section` was read from: its `_file` sibling
/// when that is set, the inline key otherwise.
fn source_key(section: &str, name: &str, from_file: bool) -> String {
    if from_file {
        format!("{section}.{name}_file")
    } else {
        format!("{section}.{name}")
    }
}

/// The refusal of a basic user and password the node client cannot send,
/// naming the key the offending part was read from (RFC 7617 §2).
fn basic_refusal(section: &str, password_file: bool, source: InvalidCredentials) -> Error {
    let user = || format!("{section}.user");
    let password = || source_key(section, "password", password_file);
    match source {
        InvalidCredentials::ColonInUserId => Error::Basic {
            key: user(),
            fault: BasicFault::Colon,
            source,
        },
        InvalidCredentials::ControlCharacter(BasicPart::UserId) => Error::Basic {
            key: user(),
            fault: BasicFault::ControlCharacter,
            source,
        },
        InvalidCredentials::ControlCharacter(BasicPart::Password) => Error::Basic {
            key: password(),
            fault: BasicFault::ControlCharacter,
            source,
        },
        InvalidCredentials::NotB64Token | InvalidCredentials::NotAHeaderValue(_) => {
            Error::Authorization {
                key: password(),
                source,
            }
        }
    }
}

/// Returns the secret `key` names, inline or from its `_file` sibling.
pub(super) fn secret(
    key: &str,
    inline: Option<&str>,
    file: Option<&Path>,
) -> Result<Option<SecretString>, Error> {
    match (inline, file) {
        (Some(_), Some(_)) => Err(Error::Conflict {
            key: key.to_owned(),
        }),
        (Some(value), None) => Ok(Some(SecretString::from(value))),
        (None, Some(path)) => {
            let text = std::fs::read_to_string(path).map_err(|source| Error::Secret {
                key: format!("{key}_file"),
                path: path.to_path_buf(),
                source,
            })?;
            let value = text.trim();
            if value.is_empty() {
                return Err(Error::EmptySecret {
                    key: format!("{key}_file"),
                    path: path.to_path_buf(),
                });
            }
            Ok(Some(SecretString::from(value)))
        }
        (None, None) => Ok(None),
    }
}
