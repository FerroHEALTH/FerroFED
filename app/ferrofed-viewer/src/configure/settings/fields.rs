// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The settings each grant of a credential shows, and the TLS material a
//! node takes beside any grant (§13.1, N25).

use crate::configure::settings::{Field, Kind};

/// The settings of basic authentication.
pub(crate) const BASIC: &[Field] = &[Field {
    key: "user",
    label: "User name",
    kind: Kind::Text,
}];

/// How the gateway authenticates at a token endpoint.
pub(crate) const CLIENT_AUTH: &[(&str, &str)] = &[
    ("", "Not set"),
    (
        "private_key_jwt",
        "Signed client assertion (private_key_jwt)",
    ),
    (
        "tls_client_auth",
        "TLS client certificate (tls_client_auth)",
    ),
    (
        "self_signed_tls_client_auth",
        "Self-signed TLS client certificate",
    ),
    ("client_secret_basic", "Client secret, Basic scheme"),
    ("client_secret_post", "Client secret, in the body"),
];

/// The OAuth 2.0 grants a node takes.
pub(crate) const OAUTH2_GRANT: &[(&str, &str)] = &[
    ("", "Not set"),
    ("client_credentials", "Client credentials"),
    ("token_exchange", "Token exchange"),
];

/// The settings of an OAuth 2.0 grant.
pub(crate) const OAUTH2: &[Field] = &[
    Field {
        key: "oauth2.grant",
        label: "Grant",
        kind: Kind::Choice(OAUTH2_GRANT),
    },
    Field {
        key: "oauth2.client_auth",
        label: "Client authentication",
        kind: Kind::Choice(CLIENT_AUTH),
    },
    Field {
        key: "oauth2.token_endpoint",
        label: "Token endpoint",
        kind: Kind::Url,
    },
    Field {
        key: "oauth2.client_id",
        label: "Client id",
        kind: Kind::Text,
    },
    Field {
        key: "oauth2.scope",
        label: "Scope",
        kind: Kind::Text,
    },
    Field {
        key: "oauth2.assertion_audience",
        label: "Client assertion audience",
        kind: Kind::Choice(&[
            ("", "Not set"),
            ("token_endpoint", "The token endpoint"),
            ("issuer", "The issuer"),
        ]),
    },
    Field {
        key: "oauth2.issuer",
        label: "Issuer",
        kind: Kind::Text,
    },
    Field {
        key: "oauth2.resource",
        label: "Resource",
        kind: Kind::Text,
    },
    Field {
        key: "oauth2.audience",
        label: "Audience",
        kind: Kind::Text,
    },
    Field {
        key: "oauth2.dpop_key_file",
        label: "DPoP key file",
        kind: Kind::Path,
    },
    Field {
        key: "oauth2.tls_client_certificate_bound_access_tokens",
        label: "Bind tokens to the TLS client certificate",
        kind: Kind::Switch,
    },
];

/// The settings of a FAPI 2.0 grant.
pub(crate) const FAPI2: &[Field] = &[
    Field {
        key: "fapi2.issuer",
        label: "Issuer",
        kind: Kind::Url,
    },
    Field {
        key: "fapi2.grant",
        label: "Grant",
        kind: Kind::Choice(OAUTH2_GRANT),
    },
    Field {
        key: "fapi2.client_id",
        label: "Client id",
        kind: Kind::Text,
    },
    Field {
        key: "fapi2.client_auth",
        label: "Client authentication",
        kind: Kind::Choice(&[
            ("", "Not set (private_key_jwt)"),
            (
                "private_key_jwt",
                "Signed client assertion (private_key_jwt)",
            ),
            (
                "tls_client_auth",
                "TLS client certificate (tls_client_auth)",
            ),
            (
                "self_signed_tls_client_auth",
                "Self-signed TLS client certificate",
            ),
        ]),
    },
    Field {
        key: "fapi2.client_key_file",
        label: "Client key file",
        kind: Kind::Path,
    },
    Field {
        key: "fapi2.previous_client_key_file",
        label: "Previous client key file",
        kind: Kind::Path,
    },
    Field {
        key: "fapi2.dpop_key_file",
        label: "DPoP key file",
        kind: Kind::Path,
    },
    Field {
        key: "fapi2.tls_client_certificate_bound_access_tokens",
        label: "Bind tokens to the TLS client certificate",
        kind: Kind::Switch,
    },
    Field {
        key: "fapi2.scope",
        label: "Scope",
        kind: Kind::Text,
    },
    Field {
        key: "fapi2.authorization_details",
        label: "Authorization details (JSON)",
        kind: Kind::Text,
    },
    Field {
        key: "fapi2.resource",
        label: "Resource",
        kind: Kind::Text,
    },
    Field {
        key: "fapi2.audience",
        label: "Audience",
        kind: Kind::Text,
    },
];

/// The settings of the Nuts grant.
pub(crate) const NUTS: &[Field] = &[
    Field {
        key: "nuts.authorization_server",
        label: "Authorization server",
        kind: Kind::Url,
    },
    Field {
        key: "nuts.scope",
        label: "Scope",
        kind: Kind::Text,
    },
    Field {
        key: "nuts.client_id",
        label: "Client id",
        kind: Kind::Text,
    },
    Field {
        key: "nuts.did",
        label: "DID",
        kind: Kind::Text,
    },
    Field {
        key: "nuts.kid",
        label: "Key id",
        kind: Kind::Text,
    },
    Field {
        key: "nuts.key_file",
        label: "Key file",
        kind: Kind::Path,
    },
    Field {
        key: "nuts.dpop_key_file",
        label: "DPoP key file",
        kind: Kind::Path,
    },
];

/// The TLS settings a node's credential takes beside any grant.
pub const TLS: &[Field] = &[
    Field {
        key: "trust_roots_file",
        label: "Trust roots file",
        kind: Kind::Path,
    },
    Field {
        key: "mtls_alias_hosts",
        label: "Mutual TLS alias hosts",
        kind: Kind::List,
    },
];
