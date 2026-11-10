// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credentials of one endpoint, and every other secret, with every
//! `_file` sibling read at boot.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_engine::onward::Scope;
use ferrofed_engine::onward::grant::lane::Organisations;
use ferrohealth_secret::file::read_secret;
use ferrohealth_secret::secret::Secret;
use ferrohealth_smart::client::assertion::AssertionKey;
use ferrohealth_smart::client::keys::KeyRing;
use ferrohealth_smart::client::{ClientAuth, PresentedTls};
use ferrohealth_smart::config::credentials::{CredentialsSection, OauthSection};
use ferrohealth_smart::kind::IssuerKind;
use jsonwebtoken::Algorithm;
use openehr_federation::object::Uri;
use openehr_its::rest::client::{BasicPart, InvalidCredentials};

use crate::config::error::Error;
use crate::config::error::basic::BasicFault;
use crate::config::error::credentials::{CredentialsFault, SigningKeyError};
use crate::config::oauth::Onward;
use crate::config::public_url::PublicUrl;
use crate::config::settings::{JWKS_PATH, Scheme, SigningSettings};
use crate::config::tls::{self, TlsFault};
use crate::config::{NodeCredentials, Signing, SigningAlgorithm};
use ferrohealth_server::tls_client::TlsClient;

/// The longest lifetime a JWT the gateway signs with its `[signing]` key may
/// have.
// NOTE: no specification governs this bound: our own design, the five minutes
// a client assertion was held to before every grant moved to ferrohealth-smart.
const MAX_ASSERTION_LIFETIME: Duration = Duration::from_secs(300);

/// Returns the scheme a service's `credentials` describe, once it is known
/// to fit the `Authorization` header it is sent in.
///
/// An onward lane is refused here: a node's section alone takes one.
#[cfg(any(feature = "binding-ihe", feature = "binding-nl"))]
pub(crate) fn resolve_credentials(
    section: &str,
    credentials: &CredentialsSection,
) -> Result<Scheme, Error> {
    if credentials.onward.is_some() {
        return Err(CredentialsFault::OnwardOnService {
            section: section.to_owned(),
        }
        .into());
    }
    scheme(section, credentials, &Beside::default(), None)
}

/// What a node's section names beside its credentials table.
#[derive(Default)]
struct Beside<'a> {
    /// Whether the onward lane carries another party's organisation.
    onward_organisations: bool,
    /// The node's section and its key, which a refusal of the whole section
    /// names and the Dutch binding reads its Nuts grant from.
    node: Option<(&'a str, &'a NodeCredentials)>,
}

/// Returns what a node's `credentials` describe: the scheme, when the
/// section names one, and the TLS material the node and its authorization
/// server are reached with, when it names any.
///
/// A section may name TLS material alone, when the transport authenticates
/// the gateway. An `oauth` grant reaches its token endpoint with the same
/// material, read once, so one that authenticates by, or binds its tokens
/// to, a TLS client certificate presents the one the node is reached with
/// (RFC 8705 §2, §3).
pub(crate) fn resolve_node_credentials(
    section: &str,
    credentials: &NodeCredentials,
) -> Result<(Option<Scheme>, Option<Arc<TlsClient>>), Error> {
    let material = credentials
        .tls
        .as_ref()
        .map(|table| tls::read(&format!("{section}.tls"), table))
        .transpose()?;
    let table = format!("{section}.credentials");
    presents_certificate(&table, &credentials.credentials, material.as_deref())?;
    let presented = material
        .as_deref()
        .map(|tls| -> Result<PresentedTls, Error> {
            Ok(PresentedTls::new(
                Arc::new(tls.platform_config()?),
                tls.certificate().cloned(),
            ))
        })
        .transpose()?;
    let beside = Beside {
        onward_organisations: credentials.onward_organisations,
        node: Some((section, credentials)),
    };
    let scheme = match scheme(
        &table,
        &credentials.credentials,
        &beside,
        presented.as_ref(),
    ) {
        Ok(scheme) => node_secretless(&table, node_scope(&table, scheme)?)?,
        Err(Error::Credentials(CredentialsFault::NoScheme { .. })) if material.is_some() => {
            return Ok((None, material));
        }
        Err(refused) => return Err(refused),
    };
    Ok((Some(scheme), material))
}

/// Refuses the `oauth` table of the node at `section` when it
/// authenticates by, or binds its tokens to, a TLS client certificate and
/// `material` presents none to the node, and when `material` presents one
/// to a token endpoint or issuer that is not `https`.
///
/// # Errors
///
/// Returns [`TlsFault::WithoutClientIdentity`] naming the table, and
/// [`TlsFault::Cleartext`] naming the URL's key.
fn presents_certificate(
    section: &str,
    credentials: &CredentialsSection,
    material: Option<&TlsClient>,
) -> Result<(), Error> {
    let Some(table) = credentials.oauth.as_ref() else {
        return Ok(());
    };
    // NOTE: RFC 8705 §3, a token bound to a certificate is used only over
    // connections that present it, so the node's own identity is the one.
    let mutual = table.client_auth.is_mutual_tls() || table.certificate_bound;
    let presents = material.and_then(TlsClient::certificate).is_some();
    if mutual && !presents {
        return Err(TlsFault::WithoutClientIdentity {
            section: format!("{section}.oauth"),
        }
        .into());
    }
    // NOTE: no specification governs this: our own design; a client certificate
    // travels in the handshake, so it is presented over TLS under every profile.
    if presents {
        for (key, url) in [
            ("token_endpoint", &table.token_endpoint),
            ("issuer", &table.issuer),
        ] {
            let tls = url::Url::parse(url).is_ok_and(|parsed| parsed.scheme() == "https");
            if !url.is_empty() && !tls {
                return Err(TlsFault::Cleartext {
                    key: format!("{section}.oauth.{key}"),
                }
                .into());
            }
        }
    }
    Ok(())
}

/// Returns `scheme`, a node's, once its `oauth` grant names no client secret:
/// a node's grant authenticates with `private_key_jwt` or its TLS client
/// certificate (§13.1, N25; FAPI 2.0 Security Profile §5.3.2.1 for a grant
/// under that profile).
///
/// # Errors
///
/// Returns [`CredentialsFault::SecretForNode`] naming the key.
fn node_secretless(section: &str, scheme: Scheme) -> Result<Scheme, Error> {
    if let Scheme::Oauth { written, .. } = &scheme {
        let table = &**written;
        // NOTE: §13.1 N25; a node's grant carries the gateway's own identity, which a
        // secret shared with the node's authorization server does not prove.
        let key = if matches!(
            table.client_auth,
            ClientAuth::ClientSecretBasic | ClientAuth::ClientSecretPost
        ) {
            Some("client_auth")
        } else if table.client_secret.is_some() {
            Some("client_secret")
        } else if table.client_secret_file.is_some() {
            Some("client_secret_file")
        } else {
            None
        };
        if let Some(key) = key {
            return Err(Error::Credentials(CredentialsFault::SecretForNode {
                key: format!("{section}.oauth.{key}"),
            }));
        }
    }
    Ok(scheme)
}

/// Returns `scheme`, a node's, once the scope of its `oauth` grant reads as
/// resource scopes of the `system` compartment, the one a client credentials
/// grant to the gateway names (SMART on openEHR, master08 §Resource Scopes).
///
/// # Errors
///
/// Returns [`CredentialsFault::Scope`] naming the scope's key.
fn node_scope(section: &str, scheme: Scheme) -> Result<Scheme, Error> {
    if let Scheme::Oauth { written, .. } = &scheme
        && !written.scope.is_empty()
    {
        Scope::parse(&written.scope).map_err(|source| {
            Error::Credentials(CredentialsFault::Scope {
                key: format!("{section}.oauth.scope"),
                source,
            })
        })?;
    }
    Ok(scheme)
}

/// Returns the scheme `credentials`, the credentials table at `section`,
/// describe, with what a node's section names `beside` it.
fn scheme(
    section: &str,
    credentials: &CredentialsSection,
    beside: &Beside<'_>,
    presented: Option<&PresentedTls>,
) -> Result<Scheme, Error> {
    let password = read_secret(
        &format!("{section}.password"),
        credentials.password.clone().map(Secret::from),
        credentials.password_file.as_deref(),
    )?;
    // NOTE: no specification governs this: our own design; a refusal of the whole section
    // names the node's section, which holds the nested table and the keys beside it.
    let whole = beside.node.map_or(section, |(node, _)| node);
    #[cfg(feature = "binding-nl")]
    let bound = beside
        .node
        .is_some_and(|(_, node)| crate::config::nl::sets_onward(node));
    #[cfg(not(feature = "binding-nl"))]
    let bound = false;
    let refused = || {
        Error::Credentials(CredentialsFault::Scheme {
            section: whole.to_owned(),
        })
    };
    let user = Some(credentials.user.as_str()).filter(|user| !user.is_empty());
    let basic = user.is_some() || password.is_some();
    if credentials.onward.is_some() && (basic || bound) {
        return Err(refused());
    }
    if beside.onward_organisations && credentials.onward.is_none() {
        return Err(CredentialsFault::OnwardOrganisations {
            section: whole.to_owned(),
        }
        .into());
    }
    if credentials.onward.is_some() && credentials.oauth.is_none() {
        return Err(Error::Missing {
            key: format!("{section}.oauth"),
        });
    }
    if bound {
        if basic || credentials.oauth.is_some() {
            return Err(refused());
        }
        #[cfg(feature = "binding-nl")]
        if let Some(grant) = beside
            .node
            .and_then(|(node_section, node)| crate::config::nl::onward(node_section, node))
        {
            return grant.map(Scheme::Binding);
        }
    }
    if let Some(oauth) = &credentials.oauth {
        if basic {
            return Err(refused());
        }
        return oauth_scheme(section, oauth, credentials, beside, presented);
    }
    match (user, password) {
        (Some(user), Some(password)) => {
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
        (Some(_), None) => Err(Error::Missing {
            key: format!("{section}.password"),
        }),
        (None, Some(_)) => Err(Error::Missing {
            key: format!("{section}.user"),
        }),
        (None, None) => Err(Error::Credentials(CredentialsFault::NoScheme {
            section: section.to_owned(),
        })),
    }
}

/// Returns the scheme of `section`'s `oauth` table, with the onward lane it
/// authenticates when `credentials` write one, carrying another party's
/// organisation when the node's section opts in.
fn oauth_scheme(
    section: &str,
    oauth: &OauthSection,
    credentials: &CredentialsSection,
    beside: &Beside<'_>,
    presented: Option<&PresentedTls>,
) -> Result<Scheme, Error> {
    let grant = crate::config::oauth::resolve(&format!("{section}.oauth"), oauth, presented)?;
    let onward = credentials
        .onward
        .as_ref()
        .map(|written| Onward::resolve(&format!("{section}.onward"), written, grant.clone()))
        .transpose()?;
    let onward = match (onward, beside.onward_organisations) {
        (Some(onward), true) if onward.kind == IssuerKind::Federation => Some(Onward {
            organisations: Organisations::ReadOnly,
            ..onward
        }),
        (_, true) => {
            return Err(CredentialsFault::OnwardOrganisations {
                section: beside.node.map_or(section, |(node, _)| node).to_owned(),
            }
            .into());
        }
        (onward, false) => onward,
    };
    Ok(Scheme::Oauth {
        grant: Box::new(grant),
        written: Box::new(oauth.clone()),
        onward: onward.map(Box::new),
    })
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
        InvalidCredentials::ColonInUserId => Error::Credentials(CredentialsFault::Basic {
            key: user(),
            fault: BasicFault::Colon,
            source,
        }),
        InvalidCredentials::ControlCharacter(BasicPart::UserId) => {
            Error::Credentials(CredentialsFault::Basic {
                key: user(),
                fault: BasicFault::ControlCharacter,
                source,
            })
        }
        InvalidCredentials::ControlCharacter(BasicPart::Password) => {
            Error::Credentials(CredentialsFault::Basic {
                key: password(),
                fault: BasicFault::ControlCharacter,
                source,
            })
        }
        InvalidCredentials::NotB64Token
        | InvalidCredentials::NotToken68
        | InvalidCredentials::NotAHeaderValue(_) => {
            Error::Credentials(CredentialsFault::Authorization {
                key: password(),
                source,
            })
        }
    }
}

/// Returns the signing keys and their publication `[signing]` describes:
/// every key read from its file, each a P-256 (ES256) or a P-384 (ES384)
/// key, the next key on the curve of the algorithm it is meant for, the
/// overlap window at least [`MAX_ASSERTION_LIFETIME`] plus the nodes' JWK Set
/// cache time, and `jwks_uri` an
/// absolute `http` or `https` URL (§13.1, N25). With `public` set,
/// `jwks_uri` defaults to the route the gateway serves the JWK Set at under
/// it, and must name that route when set.
pub(crate) fn resolve_signing(
    signing: &Signing,
    public: Option<&PublicUrl>,
) -> Result<SigningSettings, Error> {
    let max = MAX_ASSERTION_LIFETIME.as_secs();
    // NOTE: no specification governs this: our own design; a node holding the
    // previous set still meets a token the previous key signed.
    if signing.rotation_overlap.as_secs() < max.saturating_add(signing.node_jwks_cache.as_secs()) {
        return Err(Error::Credentials(CredentialsFault::RotationOverlap {
            overlap_s: signing.rotation_overlap.as_secs(),
            lifetime_s: max,
            cache_s: signing.node_jwks_cache.as_secs(),
        }));
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
    let overlap = Duration::from_secs(signing.rotation_overlap.as_secs());
    let ring_refused = |key: &str| {
        let key = key.to_owned();
        move |source| {
            Error::Credentials(CredentialsFault::SigningKey {
                key,
                source: SigningKeyError::Ring(source),
            })
        }
    };
    let mut keys = KeyRing::new(current);
    if let Some(previous) = previous {
        keys = keys
            .with_previous_for(previous, overlap)
            .map_err(ring_refused("signing.previous_key_file"))?;
    }
    if let Some(next) = next {
        keys = keys
            .with_next(next)
            .map_err(ring_refused("signing.next_key_file"))?;
    }
    Ok(SigningSettings {
        keys: Arc::new(keys),
        jwks_uri,
        rotation_overlap: overlap,
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
fn next_key(path: &Path, intended: Algorithm) -> Result<AssertionKey, Error> {
    let next = signing_key("signing.next_key", path)?;
    if next.algorithm() != intended {
        return Err(Error::Credentials(CredentialsFault::NextKeyAlgorithm {
            key: String::from("signing.next_key_file"),
            found: algorithm_name(next.algorithm()),
            intended: algorithm_name(intended),
        }));
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
fn signing_key(key: &str, path: &Path) -> Result<AssertionKey, Error> {
    let pem = read_secret::<Secret>(key, None, Some(path))?.ok_or_else(|| Error::Missing {
        key: format!("{key}_file"),
    })?;
    let refused = |source| {
        Error::Credentials(CredentialsFault::SigningKey {
            key: format!("{key}_file"),
            source,
        })
    };
    let read = AssertionKey::from_pem(pem.expose().as_bytes(), None, None)
        .map_err(|source| refused(SigningKeyError::Key(source)))?;
    if !matches!(read.algorithm(), Algorithm::ES256 | Algorithm::ES384) {
        return Err(refused(SigningKeyError::NotEc));
    }
    Ok(read)
}
