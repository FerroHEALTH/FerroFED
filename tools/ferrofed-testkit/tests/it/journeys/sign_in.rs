// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Sign-in: from the landing page, through the test OpenID Provider and
//! back, to a signed-in console whose browser holds one opaque `HttpOnly`
//! cookie and no token (RFC 6749 §4.1, RFC 7636, OpenID Connect Core 1.0
//! §3.1).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use ferrofed_viewer::app::PRODUCT;
use thirtyfour::SameSite;

use crate::journeys::browser::{Browser, until, with_browser};
use crate::journeys::journeys_enabled;
use crate::journeys::stack::Stack;

/// Signs the operator in from the landing page, and waits until the browser
/// is back on it with a session cookie.
pub(crate) async fn sign_in(browser: &Browser, stack: &Stack) -> Result<(), Box<dyn Error>> {
    browser.goto(&stack.url("/")).await?;
    browser.titled(PRODUCT).await?;
    browser.console_clean("the landing page").await?;
    browser.follow("Sign in").await?;
    until("the browser holds a session cookie", async || {
        Ok(browser.cookie(stack.cookie()).await?.is_some())
    })
    .await?;
    browser.at(&stack.url("/")).await?;
    browser.titled(PRODUCT).await?;
    browser.console_clean("the sign-in").await
}

#[tokio::test(flavor = "multi_thread")]
async fn an_operator_signs_in_at_the_provider_and_the_browser_holds_no_token()
-> Result<(), Box<dyn Error>> {
    if !journeys_enabled() {
        return Ok(());
    }
    let stack = Stack::start().await?;
    with_browser(async |browser| {
        sign_in(browser, &stack).await?;

        let requests = stack.provider().authorization_requests().await?;
        let [request] = requests.as_slice() else {
            return Err(format!("one authorization request, not {requests:?}").into());
        };
        let field = |name: &str| request.get(name).map(String::as_str);
        assert_eq!(Some("code"), field("response_type"));
        assert_eq!(Some("S256"), field("code_challenge_method"));
        assert_eq!(
            Some(stack.url("/auth/callback").as_str()),
            field("redirect_uri")
        );

        let cookie = browser
            .cookie(stack.cookie())
            .await?
            .ok_or("the session cookie")?;
        assert_eq!(Some(true), cookie.http_only, "{cookie:?}");
        assert_eq!(Some(SameSite::Lax), cookie.same_site, "{cookie:?}");

        browser.goto(&stack.url("/members")).await?;
        browser.text("caption", "Member endpoints").await?;
        let page = browser.driver().source().await?;
        let token = stack.provider().access_token();
        assert!(!page.contains(token), "the page holds the access token");
        for cookie in browser.driver().get_all_cookies().await? {
            assert!(
                !cookie.value.contains(token),
                "the cookie {} holds the access token",
                cookie.name
            );
        }
        browser.console_clean("a view, signed in").await
    })
    .await
}
