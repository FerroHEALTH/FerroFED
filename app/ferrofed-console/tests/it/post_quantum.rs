// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The key exchange of the console's client of the gateway: it prefers
//! X25519MLKEM768 (RFC 10024) and agrees on X25519 with a peer that lacks
//! it. The peer's certificate is trusted by no root, so each call ends after
//! the key exchange.

use std::error::Error;
use std::time::Duration;

use ferrofed_console::config::settings::GatewaySettings;
use ferrofed_console::gateway::{AccessToken, Gateway};
use ferrofed_testkit::kx::KxPeer;
use openehr_federation::options::OptionsRoot;
use rustls::NamedGroup;
use rustls::crypto::aws_lc_rs::kx_group;
use secrecy::SecretString;
use url::Url;

/// The group the console's client agreed on with `peer` in one call.
async fn agreed_by_the_console(peer: &KxPeer) -> Result<Option<NamedGroup>, Box<dyn Error>> {
    let gateway = Gateway::new(&GatewaySettings {
        base: Url::parse(&format!("https://localhost:{}/", peer.address().port()))?,
        timeout: Duration::from_secs(10),
    })?;
    let read = gateway
        .report::<OptionsRoot>(&AccessToken::new(SecretString::from("synthetic-token")))
        .await;
    assert!(
        read.is_err(),
        "the peer's certificate is trusted by no root"
    );
    Ok(peer
        .agreed(0)
        .map_err(|seen| format!("the peer recorded {seen} connections"))?)
}

#[tokio::test]
async fn the_console_client_prefers_x25519mlkem768() -> Result<(), Box<dyn Error>> {
    let peer = KxPeer::start(vec![kx_group::X25519, kx_group::X25519MLKEM768])?;
    assert_eq!(
        Some(NamedGroup::X25519MLKEM768),
        agreed_by_the_console(&peer).await?
    );
    Ok(())
}

#[tokio::test]
async fn the_console_client_agrees_on_x25519_with_a_classical_only_peer()
-> Result<(), Box<dyn Error>> {
    let peer = KxPeer::start(vec![kx_group::X25519])?;
    assert_eq!(
        Some(NamedGroup::X25519),
        agreed_by_the_console(&peer).await?
    );
    Ok(())
}
