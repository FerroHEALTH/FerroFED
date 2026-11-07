// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The committer of a `CONTRIBUTION` a caller sends through an onward lane.
//!
//! A node reached through an onward lane authenticates the onward token, so
//! it derives the committer of every commit from that token and takes, where
//! ITS-REST requires a committer, one placeholder naming that token's `iss`
//! and `sub`, which it recognises by its shape and discards. A
//! `CONTRIBUTION`'s `audit` and every member's `commit_audit` require one
//! (ITS-REST 1.1.0 `UpdateAudit`, `UpdateVersion`), so the gateway writes the
//! placeholder there, read from the onward token the lane obtained for the
//! caller; it writes nothing else of the body. An attestation (ITS-REST
//! `UpdateAttestation`) takes the same placeholder for a person exchanged
//! at a `persons` issuer, and is refused before anything is sent for any
//! other caller, whose attestation the node refuses. A header commit is
//! forwarded as the client sent it, and the gateway adds no committer to
//! it. The placeholder's form is FerroEHR's; which body members the gateway
//! writes is our own design.

use std::collections::BTreeMap;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ferrohealth_smart::kind::IssuerKind;
use openehr_its::rest::client::{Credentials, CredentialsError, Transport};
use secrecy::ExposeSecret as _;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::conveyance::caller::Principal;
use crate::dispatch::{DispatchOptions, NodeClient, SharedCredentials};
use crate::single_node::forward::{ForwardError, HeldRequest};

/// A JSON object whose members are kept as received, each its raw text.
type Members = BTreeMap<String, Box<RawValue>>;

/// The ITS-REST operation whose body names its committers.
const CONTRIBUTION_CREATE: &str = "contribution_create";

/// Why a `CONTRIBUTION` could not be given its committer, so nothing was
/// sent.
///
/// No variant carries a token or a body value.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CommitterError {
    /// The lane gave no onward token to read the committer from.
    #[error("the onward lane gave no token to name the committer by")]
    Credentials(#[source] CredentialsError),
    /// The onward token is no JWT whose payload names an `iss` and a `sub`.
    #[error("the onward token names no iss and sub to name the committer by")]
    Unnamed,
    /// The body is no JSON object of a `CONTRIBUTION`.
    #[error("the CONTRIBUTION is no JSON object")]
    Body(#[source] Option<serde_json::Error>),
    /// A version carries an attestation and the caller is no person of the
    /// node's organisation, whose attestation the node refuses.
    #[error("a CONTRIBUTION of a caller exchanged at a {issuer} issuer carries an attestation")]
    Attestation {
        /// The kind of the lane's issuer.
        issuer: IssuerKind,
    },
}

/// Returns `body`, the body of a request for `operation_id` sent through an
/// onward lane whose issuer signs tokens of `kind`, with the committer the
/// onward token `provider` gives names written into it, or `None` when the
/// request takes no committer of the gateway's.
///
/// # Errors
///
/// Returns the [`CommitterError`] that kept the committer from being
/// written; the request is then never sent.
pub(crate) async fn committed(
    (kind, provider): (IssuerKind, SharedCredentials),
    operation_id: &str,
    body: &[u8],
    options: &DispatchOptions,
) -> Result<Option<Vec<u8>>, CommitterError> {
    if operation_id != CONTRIBUTION_CREATE
        || matches!(options.conveyance().principal(), Principal::Gateway)
    {
        return Ok(None);
    }
    let mut contribution: Members =
        serde_json::from_slice(body).map_err(|source| CommitterError::Body(Some(source)))?;
    let mut versions: Vec<Members> = match contribution.get("versions") {
        Some(raw) => {
            serde_json::from_str(raw.get()).map_err(|source| CommitterError::Body(Some(source)))?
        }
        None => Vec::new(),
    };
    let attested = versions
        .iter()
        .any(|version| version.contains_key("attestations"));
    if attested && kind != IssuerKind::Persons {
        return Err(CommitterError::Attestation { issuer: kind });
    }
    let credentials = provider
        .credentials()
        .await
        .map_err(CommitterError::Credentials)?;
    let Credentials::Bearer(token) = credentials else {
        return Err(CommitterError::Unnamed);
    };
    let placeholder = placeholder(token.expose_secret()).ok_or(CommitterError::Unnamed)?;
    write(&mut contribution, &mut versions, &placeholder)?;
    serde_json::to_vec(&contribution)
        .map(Some)
        .map_err(|source| CommitterError::Body(Some(source)))
}

/// Writes `placeholder` as the committer of `contribution`'s `audit`, and
/// of the `commit_audit` and every attestation of each of `versions`, which
/// replace the contribution's own.
fn write(
    contribution: &mut Members,
    versions: &mut [Members],
    placeholder: &RawValue,
) -> Result<(), CommitterError> {
    set_committer(contribution, "audit", placeholder)?;
    for version in versions.iter_mut() {
        set_committer(version, "commit_audit", placeholder)?;
        if let Some(raw) = version.get("attestations") {
            let mut attestations: Vec<Members> = serde_json::from_str(raw.get())
                .map_err(|source| CommitterError::Body(Some(source)))?;
            for attestation in &mut attestations {
                attestation.insert(String::from("committer"), placeholder.to_owned());
            }
            version.insert(String::from("attestations"), raw_of(&attestations)?);
        }
    }
    if contribution.contains_key("versions") {
        contribution.insert(String::from("versions"), raw_of(&versions)?);
    }
    Ok(())
}

/// Sets the `committer` of the audit object `member` of `object` to
/// `placeholder`, when `object` holds one.
fn set_committer(
    object: &mut Members,
    member: &str,
    placeholder: &RawValue,
) -> Result<(), CommitterError> {
    let Some(raw) = object.get(member) else {
        return Ok(());
    };
    let mut audit: Members =
        serde_json::from_str(raw.get()).map_err(|source| CommitterError::Body(Some(source)))?;
    audit.insert(String::from("committer"), placeholder.to_owned());
    object.insert(member.to_owned(), raw_of(&audit)?);
    Ok(())
}

/// The raw JSON text of `value`.
fn raw_of(value: &impl Serialize) -> Result<Box<RawValue>, CommitterError> {
    serde_json::value::to_raw_value(value).map_err(|source| CommitterError::Body(Some(source)))
}

impl<T: Transport + Clone> NodeClient<T> {
    /// The held `request` with the committer an onward lane's token names
    /// written into its body, when the endpoint is reached through one and
    /// the request takes it ([`committed`]); a write through a lane that
    /// carries its caller read-only is refused first, before any token is
    /// asked for.
    pub(super) async fn committed(
        &self,
        mut request: HeldRequest,
        options: &DispatchOptions,
    ) -> Result<HeldRequest, ForwardError> {
        self.admit_write(
            options,
            crate::onward::grant::lane::reads(&request.operation),
        )?;
        let Some(lane) = self.committing(options) else {
            return Ok(request);
        };
        let written = committed(lane, request.operation.operation_id, &request.body, options)
            .await
            .map_err(|source| ForwardError::Committer {
                endpoint: self.endpoint().clone(),
                source,
            })?;
        if let Some(body) = written {
            request.body = body;
        }
        Ok(request)
    }
}

/// The `iss` and `sub` of an onward token's payload.
#[derive(Deserialize)]
struct Named {
    iss: String,
    sub: String,
}

/// The placeholder `PARTY_IDENTIFIED` naming a token's `iss` and `sub`.
#[derive(Serialize)]
struct Placeholder<'a> {
    #[serde(rename = "_type")]
    kind: &'static str,
    identifiers: [Identifier<'a>; 1],
}

/// The one `DV_IDENTIFIER` of a [`Placeholder`].
#[derive(Serialize)]
struct Identifier<'a> {
    #[serde(rename = "_type")]
    kind: &'static str,
    issuer: &'a str,
    id: &'a str,
}

/// The placeholder naming the `iss` and `sub` of `token`, a JWS whose
/// payload is a JSON object (RFC 7515 §7.1): `PARTY_IDENTIFIED` with one
/// `DV_IDENTIFIER` and nothing else, in the exact form the node recognises.
fn placeholder(token: &str) -> Option<Box<RawValue>> {
    let payload = URL_SAFE_NO_PAD.decode(token.split('.').nth(1)?).ok()?;
    // NOTE: no specification governs this: our own design; a token that is no JWS
    // naming both is legitimately unnamed, and the CONTRIBUTION is then refused.
    let named: Named = serde_json::from_slice(&payload).ok()?;
    serde_json::value::to_raw_value(&Placeholder {
        kind: "PARTY_IDENTIFIED",
        identifiers: [Identifier {
            kind: "DV_IDENTIFIER",
            issuer: &named.iss,
            id: &named.sub,
        }],
    })
    .ok()
}

#[cfg(test)]
mod tests {
    use super::{Members, placeholder, write};
    use base64::Engine as _;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    #[test]
    fn the_placeholder_names_the_tokens_issuer_and_subject_alone() {
        let payload = URL_SAFE_NO_PAD.encode(br#"{"iss":"https://as.example.org","sub":"p-1"}"#);
        let token = format!("e30.{payload}.sig");
        assert_eq!(
            Some(
                r#"{"_type":"PARTY_IDENTIFIED","identifiers":[{"_type":"DV_IDENTIFIER","issuer":"https://as.example.org","id":"p-1"}]}"#
            ),
            placeholder(&token)
                .as_deref()
                .map(serde_json::value::RawValue::get)
        );
        assert!(placeholder("opaque").is_none());
    }

    #[test]
    fn every_committer_of_the_body_is_written_and_nothing_else() {
        let body = r#"{"audit":{"change_type":{"value":"creation"},"committer":{"_type":"PARTY_SELF"}},"versions":[{"commit_audit":{"change_type":{"value":"creation"}},"data":{"a":1}},{"commit_audit":{"committer":{}},"attestations":[{"reason":{"value":"r"}}]}]}"#;
        let mut contribution: Members = serde_json::from_str(body).expect("a body");
        let mut versions: Vec<Members> =
            serde_json::from_str(contribution["versions"].get()).expect("versions");
        let placeholder =
            serde_json::value::RawValue::from_string(String::from("{\"p\":1}")).expect("raw");
        write(&mut contribution, &mut versions, &placeholder).expect("written");
        let written = serde_json::to_string(&contribution).expect("text");
        assert_eq!(
            r#"{"audit":{"change_type":{"value":"creation"},"committer":{"p":1}},"versions":[{"commit_audit":{"change_type":{"value":"creation"},"committer":{"p":1}},"data":{"a":1}},{"attestations":[{"committer":{"p":1},"reason":{"value":"r"}}],"commit_audit":{"committer":{"p":1}}}]}"#,
            written
        );
    }
}
