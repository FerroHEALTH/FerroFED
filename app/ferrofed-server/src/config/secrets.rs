// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credentials of one endpoint, and every other secret, with every
//! `_file` sibling read at boot.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_engine::onward::dpop::Prover;
use ferrofed_engine::onward::keys::{KeyRing, SigningKey};
use ferrofed_engine::onward::nuts::NutsGrant;
use ferrofed_engine::onward::provider::MAX_ASSERTION_LIFETIME;
use ferrofed_engine::onward::{Grant, Scope, SystemClock};
use ferrofed_registry::secret::Secret;
use nl_generic_functions::nuts_auth::error::InvalidInput;
use nl_generic_functions::nuts_auth::holder::{Did, Holder, HolderKey};
use openehr_federation::object::Uri;
use openehr_its::rest::client::{BasicPart, InvalidCredentials};
use secrecy::SecretString;
use secrecy::zeroize::Zeroizing;

use crate::config::error::{BasicFault, Error};
use crate::config::grant;
use crate::config::settings::{Scheme, SigningSettings};
use crate::config::{ClientAuth, Credentials, GrantKind, Nuts, OAuth2, Signing};

/// Returns the scheme `credentials` describes, once it is known to fit the
/// `Authorization` header it is sent in.
pub(super) fn resolve_credentials(
    section: &str,
    credentials: &Credentials,
) -> Result<Scheme, Error> {
    let token = secret(
        &format!("{section}.bearer_token"),
        credentials.bearer_token.as_ref(),
        credentials.bearer_token_file.as_deref(),
    )?;
    let password = secret(
        &format!("{section}.password"),
        credentials.password.as_ref(),
        credentials.password_file.as_deref(),
    )?;
    if let Some(fapi2) = &credentials.fapi2 {
        if token.is_some()
            || credentials.user.is_some()
            || password.is_some()
            || credentials.oauth2.is_some()
            || credentials.nuts.is_some()
        {
            return Err(Error::Scheme {
                section: section.to_owned(),
            });
        }
        return grant::resolve_fapi2(&format!("{section}.fapi2"), fapi2)
            .map(|grant| Scheme::Fapi2(Box::new(grant)));
    }
    if let Some(nuts) = &credentials.nuts {
        if token.is_some()
            || credentials.user.is_some()
            || password.is_some()
            || credentials.oauth2.is_some()
        {
            return Err(Error::Scheme {
                section: section.to_owned(),
            });
        }
        return resolve_nuts(&format!("{section}.nuts"), nuts)
            .map(|grant| Scheme::Nuts(Box::new(grant)));
    }
    if let Some(oauth2) = &credentials.oauth2 {
        if token.is_some() || credentials.user.is_some() || password.is_some() {
            return Err(Error::Scheme {
                section: section.to_owned(),
            });
        }
        return resolve_grant(&format!("{section}.oauth2"), oauth2)
            .map(|grant| Scheme::OAuth2(Box::new(grant)));
    }
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
            openehr_its::rest::client::Credentials::bearer(token.to_secret_string())
                .header_value()
                .map_err(|source| Error::Authorization { key, source })?;
            Ok(Scheme::Bearer(token))
        }
        (None, Some(user), Some(password)) => {
            openehr_its::rest::client::Credentials::basic(user, password.to_secret_string())
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
        InvalidCredentials::NotB64Token
        | InvalidCredentials::NotToken68
        | InvalidCredentials::NotAHeaderValue(_) => Error::Authorization {
            key: password(),
            source,
        },
    }
}

/// Returns the secret `key` names, inline or from its `_file` sibling, as
/// the redacting type `T` that holds it.
pub(super) fn secret<T>(
    key: &str,
    inline: Option<&T>,
    file: Option<&Path>,
) -> Result<Option<T>, Error>
where
    T: Clone + From<SecretString>,
{
    match (inline, file) {
        (Some(_), Some(_)) => Err(Error::Conflict {
            key: key.to_owned(),
        }),
        (Some(value), None) => Ok(Some(value.clone())),
        (None, Some(path)) => {
            read_secret(&format!("{key}_file"), path).map(|value| Some(T::from(value)))
        }
        (None, None) => Ok(None),
    }
}

/// Reads the secret file `path`, trimmed and refused when empty, into a
/// [`SecretString`]; the text read is zeroed once the trimmed value is taken.
fn read_secret(key: &str, path: &Path) -> Result<SecretString, Error> {
    let text = std::fs::read_to_string(path)
        .map(Zeroizing::new)
        .map_err(|source| Error::Secret {
            key: key.to_owned(),
            path: path.to_path_buf(),
            source,
        })?;
    let value = text.trim();
    if value.is_empty() {
        return Err(Error::EmptySecret {
            key: key.to_owned(),
            path: path.to_path_buf(),
        });
    }
    Ok(SecretString::from(value))
}

/// Returns the grant the `oauth2` table at `section` describes, every key
/// set and each value held to its rule: the scope to the SMART on openEHR
/// grammar, the token endpoint to an `http` or `https` URL with no userinfo,
/// the `resource` to an absolute URI (RFC 8707 §2).
fn resolve_grant(section: &str, oauth2: &OAuth2) -> Result<Grant, Error> {
    let missing = |name: &str| Error::Missing {
        key: format!("{section}.{name}"),
    };
    let exchange = match oauth2.grant {
        Some(GrantKind::ClientCredentials) => false,
        Some(GrantKind::TokenExchange) => true,
        None => return Err(missing("grant")),
    };
    match oauth2.client_auth {
        Some(ClientAuth::PrivateKeyJwt) => {}
        None => return Err(missing("client_auth")),
    }
    let token_endpoint = oauth2
        .token_endpoint
        .as_ref()
        .ok_or_else(|| missing("token_endpoint"))?;
    if oauth2.client_id.is_empty() {
        return Err(missing("client_id"));
    }
    if oauth2.scope.trim().is_empty() {
        return Err(missing("scope"));
    }
    let scope = Scope::parse(&oauth2.scope).map_err(|source| Error::Scope {
        key: format!("{section}.scope"),
        source,
    })?;
    let refused = |source| Error::Grant {
        section: section.to_owned(),
        source,
    };
    let mut grant = Grant::new(token_endpoint, oauth2.client_id.clone(), scope).map_err(refused)?;
    grant = grant::with_assertion_audience(section, oauth2, grant)?;
    if let Some(resource) = &oauth2.resource {
        grant = grant.with_resource(resource).map_err(refused)?;
    }
    if let Some(audience) = &oauth2.audience {
        grant = grant.with_audience(audience.clone()).map_err(refused)?;
    }
    if exchange {
        // NOTE: RFC 8707 §2, RFC 8693 §2.1: an exchanged token names the node it
        // is for, so a token-exchange grant without a resource is refused.
        if oauth2.resource.is_none() {
            return Err(missing("resource"));
        }
        grant = grant.with_token_exchange();
    }
    if let Some(path) = &oauth2.dpop_key_file {
        let key = format!("{section}.dpop_key");
        let pem = secret::<Secret>(&key, None, Some(path))?.ok_or_else(|| Error::Missing {
            key: format!("{key}_file"),
        })?;
        let prover =
            Prover::from_pem(&pem.to_secret_string()).map_err(|source| Error::DpopKey {
                key: format!("{key}_file"),
                source,
            })?;
        grant = grant.with_dpop(Arc::new(prover));
    }
    Ok(grant)
}

/// Returns the Nuts grant the `nuts` table at `section` describes (Annex B
/// §B.4, Nuts RFC021): the authorization server an issuer URL (RFC 8414
/// §2), the scope a list of RFC 6749 §3.3 scope tokens, the holder a
/// `did:web` DID whose key `kid` names and whose every credential is a JWT
/// credential issued to it, and a `DPoP` key the tokens are bound to.
fn resolve_nuts(section: &str, nuts: &Nuts) -> Result<NutsGrant, Error> {
    let missing = |name: &str| Error::Missing {
        key: format!("{section}.{name}"),
    };
    let server = nuts
        .authorization_server
        .as_ref()
        .ok_or_else(|| missing("authorization_server"))?;
    if nuts.scope.trim().is_empty() {
        return Err(missing("scope"));
    }
    let refused = |name: &str| {
        let key = format!("{section}.{name}");
        move |source| Error::NutsGrant { key, source }
    };
    let mut grant = nl_generic_functions::nuts_auth::Grant::new(server.expose(), &nuts.scope)
        .map_err(|source| match source {
            InvalidInput::Scope => refused("scope")(source),
            _ => refused("authorization_server")(source),
        })?;
    if let Some(client_id) = &nuts.client_id {
        grant = grant
            .with_client_id(client_id.clone())
            .map_err(refused("client_id"))?;
    }
    if nuts.did.is_empty() {
        return Err(missing("did"));
    }
    if nuts.kid.is_empty() {
        return Err(missing("kid"));
    }
    if nuts.credential.is_empty() {
        return Err(missing("credential"));
    }
    let holder_refused = |source| Error::NutsHolder {
        section: section.to_owned(),
        source,
    };
    let did = Did::new(nuts.did.clone()).map_err(holder_refused)?;
    let key_file = nuts
        .key_file
        .as_deref()
        .ok_or_else(|| missing("key_file"))?;
    let pem = secret::<Secret>(&format!("{section}.key"), None, Some(key_file))?
        .ok_or_else(|| missing("key_file"))?;
    let key =
        HolderKey::from_pem(&pem.to_secret_string(), &nuts.kid, &did).map_err(holder_refused)?;
    let mut credentials = Vec::with_capacity(nuts.credential.len());
    for (index, held) in nuts.credential.iter().enumerate() {
        let prefix = format!("{section}.credential[{index}]");
        let file = held.file.as_deref().ok_or_else(|| Error::Missing {
            key: format!("{prefix}.file"),
        })?;
        let jwt = read_secret(&format!("{prefix}.file"), file)?;
        credentials.push((held.input_descriptor.clone(), jwt));
    }
    let holder = Holder::new(did, key, credentials).map_err(holder_refused)?;
    let dpop_file = nuts
        .dpop_key_file
        .as_deref()
        .ok_or_else(|| missing("dpop_key_file"))?;
    let dpop_key = format!("{section}.dpop_key");
    let dpop_pem = secret::<Secret>(&dpop_key, None, Some(dpop_file))?
        .ok_or_else(|| missing("dpop_key_file"))?;
    let prover =
        Prover::from_pem(&dpop_pem.to_secret_string()).map_err(|source| Error::DpopKey {
            key: format!("{dpop_key}_file"),
            source,
        })?;
    Ok(NutsGrant::new(grant, Arc::new(holder), Arc::new(prover)))
}

/// Returns the signing keys and their publication `[signing]` describes:
/// both keys read from their files and held to ES384, the assertion
/// lifetime at most [`MAX_ASSERTION_LIFETIME`], the overlap window at least
/// that lifetime plus the nodes' JWK Set cache time, and `jwks_uri` an
/// absolute `http` or `https` URL (§13.1, N25).
pub(super) fn resolve_signing(signing: &Signing) -> Result<SigningSettings, Error> {
    let max = MAX_ASSERTION_LIFETIME.as_secs();
    if signing.assertion_lifetime_s == 0 || signing.assertion_lifetime_s > max {
        return Err(Error::AssertionLifetime {
            seconds: signing.assertion_lifetime_s,
            max,
        });
    }
    // NOTE: no specification governs this: our own design; a node holding the
    // previous set still meets an assertion the previous key signed.
    if signing.rotation_overlap_s
        < signing
            .assertion_lifetime_s
            .saturating_add(signing.node_jwks_cache_s)
    {
        return Err(Error::RotationOverlap {
            overlap_s: signing.rotation_overlap_s,
            lifetime_s: signing.assertion_lifetime_s,
            cache_s: signing.node_jwks_cache_s,
        });
    }
    let jwks_uri = jwks_uri(signing.jwks_uri.as_deref())?;
    let key_file = signing.key_file.as_deref().ok_or_else(|| Error::Missing {
        key: String::from("signing.key_file"),
    })?;
    let current = signing_key("signing.key", key_file)?;
    let previous = signing
        .previous_key_file
        .as_deref()
        .map(|path| signing_key("signing.previous_key", path))
        .transpose()?;
    let overlap = Duration::from_secs(signing.rotation_overlap_s);
    let keys =
        KeyRing::new(current, previous, overlap, Arc::new(SystemClock)).map_err(|source| {
            Error::SigningKey {
                key: String::from("signing.previous_key_file"),
                source,
            }
        })?;
    Ok(SigningSettings {
        keys: Arc::new(keys),
        jwks_uri,
        assertion_lifetime: Duration::from_secs(signing.assertion_lifetime_s),
    })
}

/// Returns `signing.jwks_uri`, required and an absolute `http` or `https`
/// URL with no userinfo, since every node reads it (§13.1).
fn jwks_uri(text: Option<&str>) -> Result<Uri, Error> {
    let key = || String::from("signing.jwks_uri");
    let text = text.ok_or_else(|| Error::Missing { key: key() })?;
    let parsed = url::Url::parse(text).map_err(|source| Error::Url { key: key(), source })?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(Error::HttpUrl { key: key() });
    }
    Uri::new(text).map_err(|_refused| Error::HttpUrl { key: key() })
}

/// Reads the signing key the `_file` sibling of `key` names.
fn signing_key(key: &str, path: &Path) -> Result<SigningKey, Error> {
    let pem = secret::<Secret>(key, None, Some(path))?.ok_or_else(|| Error::Missing {
        key: format!("{key}_file"),
    })?;
    SigningKey::from_pem(&pem.to_secret_string()).map_err(|source| Error::SigningKey {
        key: format!("{key}_file"),
        source,
    })
}
