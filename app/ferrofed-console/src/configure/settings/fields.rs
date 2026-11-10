// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The settings each grant of a credential shows, and the TLS material a
//! node takes beside any grant (§13.1, N25).

use crate::configure::settings::{Field, Kind};
use crate::locale::Msg;

/// The settings of basic authentication.
pub(crate) const BASIC: &[Field] = &[Field {
    key: "credentials.user",
    label: Msg::UserName,
    kind: Kind::Text,
}];

/// How the gateway authenticates at a node's token endpoint: with an
/// assertion or its TLS client certificate, never a client secret (§13.1,
/// N25).
pub(crate) const CLIENT_AUTH: &[(&str, Msg)] = &[
    ("private_key_jwt", Msg::SignedClientAssertionPrivateKeyJwt),
    ("tls_client_auth", Msg::TlsClientCertificateTlsClientAuth),
    (
        "self_signed_tls_client_auth",
        Msg::SelfSignedTlsClientCertificate,
    ),
];

/// The security profiles the client credentials grant is held to.
pub(crate) const SECURITY_PROFILE: &[(&str, Msg)] = &[
    ("", Msg::NotSet3),
    ("oauth2", Msg::Oauth20),
    ("fapi2", Msg::Fapi20),
];

/// The settings of the client credentials grant, the `oauth` table of the
/// node's nested `ferrohealth-smart` credentials table, with its sender
/// constraint and its security profile.
pub(crate) const OAUTH: &[Field] = &[
    Field {
        key: "credentials.oauth.issuer",
        label: Msg::Issuer,
        kind: Kind::Text,
    },
    Field {
        key: "credentials.oauth.token_endpoint",
        label: Msg::TokenEndpoint,
        kind: Kind::Url,
    },
    Field {
        key: "credentials.oauth.client_id",
        label: Msg::ClientId,
        kind: Kind::Text,
    },
    Field {
        key: "credentials.oauth.client_auth",
        label: Msg::ClientAuthentication,
        kind: Kind::Choice(CLIENT_AUTH),
    },
    Field {
        key: "credentials.oauth.security_profile",
        label: Msg::SecurityProfile,
        kind: Kind::Choice(SECURITY_PROFILE),
    },
    Field {
        key: "credentials.oauth.private_key_file",
        label: Msg::ClientKeyFile,
        kind: Kind::Path,
    },
    Field {
        key: "credentials.oauth.previous_private_key_file",
        label: Msg::PreviousClientKeyFile,
        kind: Kind::Path,
    },
    Field {
        key: "credentials.oauth.dpop_key_file",
        label: Msg::DpopKeyFile,
        kind: Kind::Path,
    },
    Field {
        key: "credentials.oauth.certificate_bound",
        label: Msg::BindTokensTlsClientCertificate,
        kind: Kind::Switch,
    },
    Field {
        key: "credentials.oauth.mtls_alias_hosts",
        label: Msg::MutualTlsAliasHosts,
        kind: Kind::List,
    },
    Field {
        key: "credentials.oauth.scope",
        label: Msg::Scope,
        kind: Kind::Text,
    },
    Field {
        key: "credentials.oauth.authorization_details",
        label: Msg::AuthorizationDetailsJson,
        kind: Kind::Text,
    },
    Field {
        key: "credentials.oauth.resource",
        label: Msg::Resource,
        kind: Kind::Text,
    },
    Field {
        key: "credentials.oauth.audience",
        label: Msg::Audience,
        kind: Kind::Text,
    },
];

/// The settings of the Nuts grant.
pub(crate) const NUTS: &[Field] = &[
    Field {
        key: "nuts.authorization_server",
        label: Msg::AuthorizationServer,
        kind: Kind::Url,
    },
    Field {
        key: "nuts.scope",
        label: Msg::Scope,
        kind: Kind::Text,
    },
    Field {
        key: "nuts.client_id",
        label: Msg::ClientId,
        kind: Kind::Text,
    },
    Field {
        key: "nuts.did",
        label: Msg::Did,
        kind: Kind::Text,
    },
    Field {
        key: "nuts.kid",
        label: Msg::KeyId,
        kind: Kind::Text,
    },
    Field {
        key: "nuts.key_file",
        label: Msg::KeyFile,
        kind: Kind::Path,
    },
    Field {
        key: "nuts.dpop_key_file",
        label: Msg::DpopKeyFile,
        kind: Kind::Path,
    },
];

/// The TLS settings a node's credential takes beside any grant.
pub const TLS: &[Field] = &[Field {
    key: "tls.trust_roots_file",
    label: Msg::TrustRootsFile,
    kind: Kind::Path,
}];
