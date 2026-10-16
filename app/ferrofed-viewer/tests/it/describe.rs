// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The self-description against a stub gateway: cards of what `OPTIONS
//! {base}/` declares, the manufacturer it names, and the document itself
//! highlighted behind a disclosure, every value escaped (§7a.2, N30).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use http::StatusCode;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{console, get_as, send, signed_in};
use crate::views::options_example;

/// The page `/federation` renders over a gateway answering `OPTIONS` with
/// `body`.
async fn described(body: String) -> Result<String, Box<dyn Error>> {
    let gateway = MockServer::start().await;
    Mock::given(method("OPTIONS"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(body),
        )
        .mount(&gateway)
        .await;
    let (state, service) = console(&format!(
        "[gateway]\nbase_url = \"{}\"\n\n[session]\nsecure_cookie = false\n",
        gateway.uri()
    ))?;
    let session = state.sessions().establish(signed_in())?;
    let (response, page) = send(&service, get_as("/federation", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{page}");
    Ok(page)
}

/// The §7a.2 example with `members` added at its top level.
fn with_members(members: &str) -> Result<String, Box<dyn Error>> {
    let example = options_example()?;
    let (open, rest) = example
        .split_once('{')
        .ok_or("the example is a JSON object")?;
    Ok(format!("{open}{{{members},{rest}"))
}

#[tokio::test]
async fn the_self_description_shows_its_cards_and_the_document() -> Result<(), Box<dyn Error>> {
    let page = described(with_members(
        r#""manufacturer":{"name":"Example Manufacturer B.V.","postal_address":"1 Example Street","email":"info@example.org","website":"https://example.org/"},"supported_until":"2031-10-16""#,
    )?)
    .await?;
    for shown in [
        ">Federation</h2>",
        ">Capabilities</h2>",
        ">Budgets and paging</h2>",
        ">Manufacturer</h2>",
        "Example Manufacturer B.V.",
        "1 Example Street",
        "2031-10-16",
        "<caption>Declared endpoints</caption>",
        "<summary>The OPTIONS document</summary>",
        r#"<span class="tok-key">"spec_version"</span>"#,
    ] {
        assert!(page.contains(shown), "{shown}: {page}");
    }
    Ok(())
}

#[tokio::test]
async fn a_self_description_without_a_manufacturer_says_so() -> Result<(), Box<dyn Error>> {
    let page = described(options_example()?).await?;
    assert!(page.contains("not declared"), "{page}");
    let unreadable = described(with_members(r#""manufacturer":"a string""#)?).await?;
    assert!(
        unreadable.contains("declared in a form the console cannot read"),
        "{unreadable}"
    );
    Ok(())
}

// The page reaches the browser through `inner_html`, so a value the gateway
// declares is escaped in the cards and in the document.
#[tokio::test]
async fn hostile_values_in_the_self_description_render_escaped() -> Result<(), Box<dyn Error>> {
    let page = described(with_members(
        r#""manufacturer":{"name":"<script>alert(1)</script>","website":"\"><img src=x>"}"#,
    )?)
    .await?;
    for raw in ["<script>alert(1)", "<img src=x>"] {
        assert!(!page.contains(raw), "{raw}: {page}");
    }
    assert!(
        page.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
        "{page}"
    );
    assert!(page.contains("&lt;img src=x&gt;"), "{page}");
    Ok(())
}
