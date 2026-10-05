// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Sign-out: the console's own sign-out button ends the session on the
//! server, drops the browser's cookie, and sends the operator through the
//! provider's end-session endpoint back to the landing page (OpenID Connect
//! RP-Initiated Logout 1.0 §2).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use ferrofed_viewer::app::PRODUCT;
use thirtyfour::By;

use crate::journeys::browser::{until, with_browser};
use crate::journeys::journeys_enabled;
use crate::journeys::provider::{CLIENT_ID, END_SESSION};
use crate::journeys::sign_in::sign_in;
use crate::journeys::stack::Stack;

#[tokio::test(flavor = "multi_thread")]
async fn signing_out_ends_the_session_and_returns_through_the_provider()
-> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;
        browser.goto(&stack.url("/members")).await?;
        browser.text("caption", "Member endpoints").await?;
        let session = browser
            .cookie(stack.cookie())
            .await?
            .ok_or("the session cookie")?;

        browser
            .element(By::XPath("//nav//form//button[. = 'Sign out']"))
            .await?
            .click()
            .await?;
        until("the browser has dropped the session cookie", async || {
            Ok(browser.cookie(stack.cookie()).await?.is_none())
        })
        .await?;
        browser.at(&stack.url("/")).await?;
        browser.titled(PRODUCT).await?;
        browser.element(By::LinkText("Sign in")).await?;

        let ended = stack.provider().requests_at(END_SESSION).await?;
        let [request] = ended.as_slice() else {
            return Err(format!("one end-session request, not {ended:?}").into());
        };
        let field = |name: &str| request.get(name).map(String::as_str);
        assert!(field("id_token_hint").is_some(), "{request:?}");
        assert_eq!(Some(CLIENT_ID), field("client_id"));
        assert_eq!(
            Some(stack.url("/").as_str()),
            field("post_logout_redirect_uri")
        );

        // The session the browser held is gone on the server too.
        let answer = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()?
            .get(stack.url("/members"))
            .header("cookie", format!("{}={}", session.name, session.value))
            .send()
            .await?;
        assert_eq!(reqwest::StatusCode::SEE_OTHER, answer.status());
        assert_eq!(
            Some("/login"),
            answer
                .headers()
                .get("location")
                .and_then(|location| location.to_str().ok())
        );
        browser.console_clean("the sign-out").await
    })
    .await
}
