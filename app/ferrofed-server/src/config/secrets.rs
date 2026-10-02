// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credentials of one endpoint, with every `_file` sibling read at boot.

use std::path::Path;

use http::HeaderValue;
use secrecy::{ExposeSecret as _, SecretString};

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
            bearer_header(&key, &token)?;
            Ok(Scheme::Bearer(token))
        }
        (None, Some(user), Some(password)) => {
            let user_key = format!("{section}.user");
            basic_value(&user_key, user)?;
            if user.contains(':') {
                return Err(Error::Basic {
                    key: user_key,
                    fault: BasicFault::Colon,
                });
            }
            let key = source_key(section, "password", credentials.password_file.is_some());
            basic_value(&key, password.expose_secret())?;
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

/// Refuses a bearer token whose `Authorization` value is not a legal header
/// value: `Bearer` and the token, as the node client composes it, checked by
/// the same `http` parse the client applies (RFC 6750 §2.1).
fn bearer_header(key: &str, token: &SecretString) -> Result<(), Error> {
    let composed = SecretString::from(format!("Bearer {}", token.expose_secret()));
    HeaderValue::from_str(composed.expose_secret())
        .map(drop)
        .map_err(|source| Error::Authorization {
            key: key.to_owned(),
            source,
        })
}

/// Refuses a basic user-id or password that holds a control character.
///
/// The base64 of any user and password is a legal header value, so the bound
/// here is the scheme's own: RFC 7617 §2 forbids `CTL` (RFC 5234 Appendix
/// B.1) in both.
fn basic_value(key: &str, value: &str) -> Result<(), Error> {
    if value.chars().any(|c| c.is_ascii_control()) {
        return Err(Error::Basic {
            key: key.to_owned(),
            fault: BasicFault::ControlCharacter,
        });
    }
    Ok(())
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
