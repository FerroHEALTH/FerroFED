// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The settings and secrets of a credential or a binding section, as the
//! forms show them and as the gateway takes them.
//!
//! A credential's form shows the fields of the grant the operator chooses:
//! a bearer token, basic authentication, an OAuth 2.0 grant, a FAPI 2.0
//! grant or the Nuts grant, beside the TLS material a node takes (§13.1,
//! N25). A binding section's form shows each setting it holds as a field
//! of its own, and takes new ones a row each. Either is sent as the one
//! TOML inline table the gateway takes, built here from the held settings,
//! so a key the form does not show is kept. A secret is never a setting:
//! each is kept, set by a value or a file, or cleared, one choice per
//! secret. No specification governs this: our own design.

pub mod build;
pub mod fields;

use ferrofed_registry::resource::SecretSlot;

use crate::locale::Msg;

/// How a credential reaches its endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grant {
    /// The TLS client certificate alone, or nothing.
    Transport,
    /// An RFC 6750 bearer token.
    Bearer,
    /// RFC 7617 basic authentication.
    Basic,
    /// An OAuth 2.0 grant at the node's token endpoint.
    OAuth2,
    /// A grant under the FAPI 2.0 Security Profile.
    Fapi2,
    /// The Nuts grant of the Dutch Generic Functions.
    Nuts,
}

impl Grant {
    /// Every grant, in the order the select lists them.
    pub const ALL: [Self; 6] = [
        Self::OAuth2,
        Self::Fapi2,
        Self::Nuts,
        Self::Bearer,
        Self::Basic,
        Self::Transport,
    ];

    /// The grant as the form names it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Transport => "transport",
            Self::Bearer => "bearer",
            Self::Basic => "basic",
            Self::OAuth2 => "oauth2",
            Self::Fapi2 => "fapi2",
            Self::Nuts => "nuts",
        }
    }

    /// The grant as the select shows it.
    #[must_use]
    pub const fn label(self) -> Msg {
        match self {
            Self::Transport => Msg::TlsClientCertificateOnly,
            Self::Bearer => Msg::BearerToken,
            Self::Basic => Msg::BasicAuthentication,
            Self::OAuth2 => Msg::Oauth20,
            Self::Fapi2 => Msg::Fapi20,
            Self::Nuts => Msg::Nuts,
        }
    }

    /// The grant `name` names, as [`Grant::as_str`] writes it.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|grant| grant.as_str() == name)
    }

    /// The grant the held settings `table` and secrets `slots` use.
    #[must_use]
    pub fn held(table: &toml::Table, slots: &[SecretSlot]) -> Self {
        let secret = |key: &str| slots.iter().any(|slot| slot.key == key);
        if table.contains_key("oauth2") {
            Self::OAuth2
        } else if table.contains_key("fapi2") {
            Self::Fapi2
        } else if table.contains_key("nuts") {
            Self::Nuts
        } else if table.contains_key("user") || secret("password") {
            Self::Basic
        } else if secret("bearer_token") {
            Self::Bearer
        } else if table.is_empty() && slots.is_empty() {
            Self::OAuth2
        } else {
            Self::Transport
        }
    }

    /// The top-level keys of the settings only this grant uses.
    pub(crate) const fn own(self) -> &'static [&'static str] {
        match self {
            Self::Transport | Self::Bearer => &[],
            Self::Basic => &["user"],
            Self::OAuth2 => &["oauth2"],
            Self::Fapi2 => &["fapi2"],
            Self::Nuts => &["nuts"],
        }
    }

    /// The secrets only this grant uses.
    #[must_use]
    pub const fn secrets(self) -> &'static [Secret] {
        match self {
            Self::Bearer => &[Secret {
                key: "bearer_token",
                label: Msg::BearerToken,
            }],
            Self::Basic => &[Secret {
                key: "password",
                label: Msg::Password,
            }],
            Self::OAuth2 => &[Secret {
                key: "oauth2.client_secret",
                label: Msg::ClientSecret,
            }],
            Self::Transport | Self::Fapi2 | Self::Nuts => &[],
        }
    }

    /// The settings this grant shows.
    #[must_use]
    pub const fn fields(self) -> &'static [Field] {
        match self {
            Self::Transport | Self::Bearer => &[],
            Self::Basic => fields::BASIC,
            Self::OAuth2 => fields::OAUTH2,
            Self::Fapi2 => fields::FAPI2,
            Self::Nuts => fields::NUTS,
        }
    }
}

/// A secret a form offers a choice for: its dotted key and its label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Secret {
    /// The dotted key inside the resource.
    pub key: &'static str,
    /// What the form calls it, by its message in the catalog.
    pub label: Msg,
}

/// The secret every node's credential may take beside its grant's.
pub const TLS_SECRET: Secret = Secret {
    key: "client_identity",
    label: Msg::TlsClientCertificateKeyPem,
};

/// What a setting's field takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// One line of text.
    Text,
    /// An absolute URL.
    Url,
    /// The absolute path of a file the gateway reads.
    Path,
    /// A switch, on or off.
    Switch,
    /// One of the values listed, each with its label; the first is unset.
    Choice(&'static [(&'static str, Msg)]),
    /// A list of values, each entered on its own.
    List,
}

/// One setting a credential's form shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    /// The dotted key inside the resource.
    pub key: &'static str,
    /// What the form calls it, by its message in the catalog.
    pub label: Msg,
    /// What it takes.
    pub kind: Kind,
}
