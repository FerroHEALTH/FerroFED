// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The settings each grant of a credential shows, and the TLS material a
//! node takes beside any grant (§13.1, N25).

use crate::configure::settings::{Field, Kind};
use crate::locale::Msg;

/// The settings of basic authentication.
pub(crate) const BASIC: &[Field] = &[Field {
    key: "user",
    label: Msg::UserName,
    kind: Kind::Text,
}];

/// How the gateway authenticates at a token endpoint.
pub(crate) const CLIENT_AUTH: &[(&str, Msg)] = &[
    ("", Msg::NotSet3),
    ("private_key_jwt", Msg::SignedClientAssertionPrivateKeyJwt),
    ("tls_client_auth", Msg::TlsClientCertificateTlsClientAuth),
    (
        "self_signed_tls_client_auth",
        Msg::SelfSignedTlsClientCertificate,
    ),
    ("client_secret_basic", Msg::ClientSecretBasicScheme),
    ("client_secret_post", Msg::ClientSecretBody),
];

/// The OAuth 2.0 grants a node takes.
pub(crate) const OAUTH2_GRANT: &[(&str, Msg)] = &[
    ("", Msg::NotSet3),
    ("client_credentials", Msg::ClientCredentials),
    ("token_exchange", Msg::TokenExchange),
];

/// The settings of an OAuth 2.0 grant.
pub(crate) const OAUTH2: &[Field] = &[
    Field {
        key: "oauth2.grant",
        label: Msg::Grant,
        kind: Kind::Choice(OAUTH2_GRANT),
    },
    Field {
        key: "oauth2.client_auth",
        label: Msg::ClientAuthentication,
        kind: Kind::Choice(CLIENT_AUTH),
    },
    Field {
        key: "oauth2.token_endpoint",
        label: Msg::TokenEndpoint,
        kind: Kind::Url,
    },
    Field {
        key: "oauth2.client_id",
        label: Msg::ClientId,
        kind: Kind::Text,
    },
    Field {
        key: "oauth2.scope",
        label: Msg::Scope,
        kind: Kind::Text,
    },
    Field {
        key: "oauth2.assertion_audience",
        label: Msg::ClientAssertionAudience,
        kind: Kind::Choice(&[
            ("", Msg::NotSet3),
            ("token_endpoint", Msg::TheTokenEndpoint),
            ("issuer", Msg::TheIssuer),
        ]),
    },
    Field {
        key: "oauth2.issuer",
        label: Msg::Issuer,
        kind: Kind::Text,
    },
    Field {
        key: "oauth2.resource",
        label: Msg::Resource,
        kind: Kind::Text,
    },
    Field {
        key: "oauth2.audience",
        label: Msg::Audience,
        kind: Kind::Text,
    },
    Field {
        key: "oauth2.dpop_key_file",
        label: Msg::DpopKeyFile,
        kind: Kind::Path,
    },
    Field {
        key: "oauth2.tls_client_certificate_bound_access_tokens",
        label: Msg::BindTokensTlsClientCertificate,
        kind: Kind::Switch,
    },
];

/// The settings of a FAPI 2.0 grant.
pub(crate) const FAPI2: &[Field] = &[
    Field {
        key: "fapi2.issuer",
        label: Msg::Issuer,
        kind: Kind::Url,
    },
    Field {
        key: "fapi2.grant",
        label: Msg::Grant,
        kind: Kind::Choice(OAUTH2_GRANT),
    },
    Field {
        key: "fapi2.client_id",
        label: Msg::ClientId,
        kind: Kind::Text,
    },
    Field {
        key: "fapi2.client_auth",
        label: Msg::ClientAuthentication,
        kind: Kind::Choice(&[
            ("", Msg::NotSetPrivateKeyJwt),
            ("private_key_jwt", Msg::SignedClientAssertionPrivateKeyJwt),
            ("tls_client_auth", Msg::TlsClientCertificateTlsClientAuth),
            (
                "self_signed_tls_client_auth",
                Msg::SelfSignedTlsClientCertificate,
            ),
        ]),
    },
    Field {
        key: "fapi2.client_key_file",
        label: Msg::ClientKeyFile,
        kind: Kind::Path,
    },
    Field {
        key: "fapi2.previous_client_key_file",
        label: Msg::PreviousClientKeyFile,
        kind: Kind::Path,
    },
    Field {
        key: "fapi2.dpop_key_file",
        label: Msg::DpopKeyFile,
        kind: Kind::Path,
    },
    Field {
        key: "fapi2.tls_client_certificate_bound_access_tokens",
        label: Msg::BindTokensTlsClientCertificate,
        kind: Kind::Switch,
    },
    Field {
        key: "fapi2.scope",
        label: Msg::Scope,
        kind: Kind::Text,
    },
    Field {
        key: "fapi2.authorization_details",
        label: Msg::AuthorizationDetailsJson,
        kind: Kind::Text,
    },
    Field {
        key: "fapi2.resource",
        label: Msg::Resource,
        kind: Kind::Text,
    },
    Field {
        key: "fapi2.audience",
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
pub const TLS: &[Field] = &[
    Field {
        key: "trust_roots_file",
        label: Msg::TrustRootsFile,
        kind: Kind::Path,
    },
    Field {
        key: "mtls_alias_hosts",
        label: Msg::MutualTlsAliasHosts,
        kind: Kind::List,
    },
];
