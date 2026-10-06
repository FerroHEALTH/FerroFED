// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credentials of one endpoint, and every other secret, with every
//! `_file` sibling read at boot.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_engine::onward::dpop::Prover;
use ferrofed_engine::onward::keys::{KeyRing, SigningKey};
use ferrofed_engine::onward::mtls::Thumbprint;
use ferrofed_engine::onward::token::MAX_ASSERTION_LIFETIME;
use ferrofed_engine::onward::{ClientAuthentication, Grant, Scope, SystemClock};
use ferrofed_registry::secret::Secret;
use jsonwebtoken::Algorithm;
use openehr_federation::object::Uri;
use openehr_its::rest::client::{BasicPart, InvalidCredentials};
use secrecy::SecretString;
use secrecy::zeroize::Zeroizing;

use crate::binding::Binding;
use crate::config::error::Error;
use crate::config::error::basic::BasicFault;
use crate::config::grant;
use crate::config::public_url::PublicUrl;
use crate::config::settings::{Scheme, SigningSettings};
use crate::config::tls::{self, TlsFault, TlsSettings};
use crate::config::{Credentials, GrantKind, OAuth2, Signing, SigningAlgorithm};
use crate::jwks::JWKS_PATH;

/// Returns the scheme a service's `credentials` describe, once it is known
/// to fit the `Authorization` header it is sent in.
///
/// TLS material is refused here: a service's own table names it.
#[cfg(any(feature = "binding-ihe", feature = "binding-nl"))]
pub(crate) fn resolve_credentials(
    section: &str,
    credentials: &Credentials,
) -> Result<Scheme, Error> {
    if credentials.names_tls() {
        return Err(TlsFault::OnService {
            section: section.to_owned(),
        }
        .into());
    }
    scheme(section, credentials, None)
}

/// Returns what a node's `credentials` describe: the scheme, when the
/// section names one, and the TLS material the node and its authorization
/// server are reached with, when it names any.
///
/// A section may name TLS material alone, when the transport authenticates
/// the gateway. A grant that uses mutual TLS reads the thumbprint of the
/// client certificate (RFC 8705 §3.1).
pub(crate) fn resolve_node_credentials(
    section: &str,
    credentials: &Credentials,
) -> Result<(Option<Scheme>, Option<TlsSettings>), Error> {
    if !credentials.names_tls() {
        return scheme(section, credentials, None).map(|scheme| (Some(scheme), None));
    }
    let material = tls::resolve(
        section,
        credentials.client_identity.as_ref(),
        credentials.client_identity_file.as_deref(),
        credentials.trust_roots_file.as_ref(),
    )?;
    let thumbprint = material
        .client_identity
        .as_ref()
        .map(|identity| {
            Thumbprint::of_identity(&identity.to_secret_string()).map_err(|source| {
                TlsFault::Identity {
                    key: source_key(
                        section,
                        "client_identity",
                        credentials.client_identity_file.is_some(),
                    ),
                    source,
                }
            })
        })
        .transpose()?;
    match scheme(section, credentials, thumbprint.as_ref()) {
        Ok(scheme) => Ok((Some(scheme), Some(material))),
        Err(Error::NoScheme { .. }) => Ok((None, Some(material))),
        Err(refused) => Err(refused),
    }
}

/// Returns the scheme `credentials` describe, with `certificate` the
/// thumbprint of the node's TLS client certificate when it has one.
fn scheme(
    section: &str,
    credentials: &Credentials,
    certificate: Option<&Thumbprint>,
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
    let bound: Vec<&dyn Binding> = crate::binding::compiled()
        .iter()
        .copied()
        .filter(|binding| binding.onward_table(credentials).is_some())
        .collect();
    let refused = || Error::Scheme {
        section: section.to_owned(),
    };
    if let Some(fapi2) = &credentials.fapi2 {
        if token.is_some()
            || credentials.user.is_some()
            || password.is_some()
            || credentials.oauth2.is_some()
            || !bound.is_empty()
        {
            return Err(refused());
        }
        let resolved = grant::resolve_fapi2(&format!("{section}.fapi2"), fapi2, certificate)?;
        return grant::with_mtls_alias_hosts(section, &credentials.mtls_alias_hosts, resolved)
            .map(|grant| Scheme::Fapi2(Box::new(grant)));
    }
    if !credentials.mtls_alias_hosts.is_empty() {
        return Err(grant::GrantFault::AliasHostsUnused {
            key: format!("{section}.mtls_alias_hosts"),
        }
        .into());
    }
    if let [binding] = bound.as_slice() {
        if token.is_some()
            || credentials.user.is_some()
            || password.is_some()
            || credentials.oauth2.is_some()
        {
            return Err(refused());
        }
        if let Some(grant) = binding.onward(section, credentials) {
            return grant.map(Scheme::Binding);
        }
    }
    if bound.len() > 1 {
        return Err(refused());
    }
    if let Some(oauth2) = &credentials.oauth2 {
        if token.is_some() || credentials.user.is_some() || password.is_some() {
            return Err(Error::Scheme {
                section: section.to_owned(),
            });
        }
        return resolve_grant(&format!("{section}.oauth2"), oauth2, certificate)
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
pub(crate) fn secret<T>(
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
pub(crate) fn read_secret(key: &str, path: &Path) -> Result<SecretString, Error> {
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
/// the `resource` to an absolute URI (RFC 8707 §2). A grant that authenticates
/// by, or binds its tokens to, the TLS client certificate of `certificate`
/// reaches an `https` token endpoint alone (RFC 8705).
fn resolve_grant(
    section: &str,
    oauth2: &OAuth2,
    certificate: Option<&Thumbprint>,
) -> Result<Grant, Error> {
    let missing = |name: &str| Error::Missing {
        key: format!("{section}.{name}"),
    };
    let exchange = match oauth2.grant {
        Some(GrantKind::ClientCredentials) => false,
        Some(GrantKind::TokenExchange) => true,
        None => return Err(missing("grant")),
    };
    if oauth2.client_secret.is_some() || oauth2.client_secret_file.is_some() {
        return Err(grant::GrantFault::SecretForNode {
            key: source_key(
                section,
                "client_secret",
                oauth2.client_secret_file.is_some(),
            ),
        }
        .into());
    }
    let client_auth = grant::client_authentication(
        section,
        oauth2.client_auth.ok_or_else(|| missing("client_auth"))?,
        certificate,
    )?;
    let bound = grant::certificate_binding(
        section,
        (
            oauth2.tls_client_certificate_bound_access_tokens,
            oauth2.dpop_key_file.is_some(),
        ),
        certificate,
    )?;
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
    if matches!(client_auth, ClientAuthentication::Tls(_)) || bound.is_some() {
        grant::mutual_tls_url(
            &format!("{section}.token_endpoint"),
            token_endpoint.expose(),
        )?;
    }
    let mut grant = Grant::new(token_endpoint, oauth2.client_id.clone(), scope).map_err(refused)?;
    if let ClientAuthentication::Tls(method) = client_auth {
        grant = grant.with_tls_client_auth(method);
    }
    if let Some(thumbprint) = bound {
        grant = grant.with_certificate_binding(thumbprint);
    }
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

/// Returns the signing keys and their publication `[signing]` describes:
/// every key read from its file, each a P-256 (ES256) or a P-384 (ES384)
/// key, the next key on the curve of the algorithm it is meant for, the
/// assertion lifetime at most [`MAX_ASSERTION_LIFETIME`], the overlap window at least
/// that lifetime plus the nodes' JWK Set cache time, and `jwks_uri` an
/// absolute `http` or `https` URL (§13.1, N25). With `public` set,
/// `jwks_uri` defaults to the route the gateway serves the JWK Set at under
/// it, and must name that route when set.
pub(crate) fn resolve_signing(
    signing: &Signing,
    public: Option<&PublicUrl>,
) -> Result<SigningSettings, Error> {
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
    let jwks_uri = jwks_uri(signing.jwks_uri.as_deref(), public)?;
    let key_file = signing.key_file.as_deref().ok_or_else(|| Error::Missing {
        key: String::from("signing.key_file"),
    })?;
    let current = signing_key("signing.key", key_file)?;
    let previous = signing
        .previous_key_file
        .as_deref()
        .map(|path| signing_key("signing.previous_key", path))
        .transpose()?;
    let intended = signing
        .next_key_algorithm
        .map_or_else(|| current.algorithm(), SigningAlgorithm::algorithm);
    let next = signing
        .next_key_file
        .as_deref()
        .map(|path| next_key(path, intended))
        .transpose()?;
    if signing.next_key_algorithm.is_some() && next.is_none() {
        return Err(Error::Missing {
            key: String::from("signing.next_key_file"),
        });
    }
    let overlap = Duration::from_secs(signing.rotation_overlap_s);
    let mut keys =
        KeyRing::new(current, previous, overlap, Arc::new(SystemClock)).map_err(|source| {
            Error::SigningKey {
                key: String::from("signing.previous_key_file"),
                source,
            }
        })?;
    if let Some(next) = next {
        keys = keys.with_next(next).map_err(|source| Error::SigningKey {
            key: String::from("signing.next_key_file"),
            source,
        })?;
    }
    Ok(SigningSettings {
        keys: Arc::new(keys),
        jwks_uri,
        assertion_lifetime: Duration::from_secs(signing.assertion_lifetime_s),
    })
}

/// Returns `signing.jwks_uri`, an absolute `http` or `https` URL with no
/// userinfo, since every node reads it (§13.1): as written, or the JWK Set
/// route under `public` when unset, and refused when it names another.
fn jwks_uri(text: Option<&str>, public: Option<&PublicUrl>) -> Result<Uri, Error> {
    const KEY: &str = "signing.jwks_uri";
    let key = || String::from(KEY);
    let text = match (text, public) {
        (Some(text), _) => text.to_owned(),
        (None, Some(public)) => public.route(KEY, JWKS_PATH)?.to_string(),
        (None, None) => return Err(Error::Missing { key: key() }),
    };
    let parsed = url::Url::parse(&text).map_err(|source| Error::Url { key: key(), source })?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(Error::HttpUrl { key: key() });
    }
    if let Some(public) = public {
        public.agrees(KEY, &parsed, JWKS_PATH)?;
    }
    Uri::new(&text).map_err(|_refused| Error::HttpUrl { key: key() })
}

/// Reads the next signing key from `path`, refused when its curve signs
/// another algorithm than `intended` (RFC 7518 §3.4).
fn next_key(path: &Path, intended: Algorithm) -> Result<SigningKey, Error> {
    let next = signing_key("signing.next_key", path)?;
    if next.algorithm() != intended {
        return Err(Error::NextKeyAlgorithm {
            key: String::from("signing.next_key_file"),
            found: algorithm_name(next.algorithm()),
            intended: algorithm_name(intended),
        });
    }
    Ok(next)
}

/// The name RFC 7518 §3.1 gives `algorithm`, one of the two a signing key
/// signs with.
fn algorithm_name(algorithm: Algorithm) -> &'static str {
    match algorithm {
        Algorithm::ES256 => "ES256",
        Algorithm::ES384 => "ES384",
        _ => "an algorithm other than ES256 and ES384",
    }
}

/// Reads the signing key the `_file` sibling of `key` names.
fn signing_key(key: &str, path: &Path) -> Result<SigningKey, Error> {
    let pem = secret::<Secret>(key, None, Some(path))?.ok_or_else(|| Error::Missing {
        key: format!("{key}_file"),
    })?;
    SigningKey::from_ec_pem(&pem.to_secret_string()).map_err(|source| Error::SigningKey {
        key: format!("{key}_file"),
        source,
    })
}
