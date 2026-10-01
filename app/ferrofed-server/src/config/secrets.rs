// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credentials of one endpoint, with every `_file` sibling read at boot.

use std::path::Path;

use secrecy::SecretString;

use crate::config::Credentials;
use crate::config::error::Error;
use crate::config::settings::Scheme;

/// Returns the scheme `credentials` describes.
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
        (Some(token), None, None) => Ok(Scheme::Bearer(token)),
        (None, Some(user), Some(password)) => Ok(Scheme::Basic {
            user: user.to_owned(),
            password,
        }),
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

/// Returns the secret `key` names, inline or from its `_file` sibling.
fn secret(
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
