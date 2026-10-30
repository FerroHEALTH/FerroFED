// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The self-description against a stub gateway: the tabs of what `OPTIONS
//! {base}/` declares and the manufacturer it names, every value escaped
//! (§7a.2, N30).

use std::error::Error;

use http::StatusCode;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::stub::options_example;
use crate::support::{console, get_as, send, signed_in};

/// The page `/self-description` renders over a gateway answering `OPTIONS` with
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
    let (response, page) = send(&service, get_as("/self-description", &session)?).await?;
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
        r#"href="/self-description/capabilities""#,
        r#"href="/self-description/limits""#,
        r#"href="/self-description/endpoints""#,
        "Manufacturer",
        "Example Manufacturer B.V.",
        "1 Example Street",
        "info@example.org",
        "2031-10-16",
    ] {
        assert!(page.contains(shown), "{shown}: {page}");
    }
    Ok(())
}

#[tokio::test]
async fn a_self_description_without_a_manufacturer_says_so() -> Result<(), Box<dyn Error>> {
    let page = described(options_example()?).await?;
    assert!(page.contains("Not declared"), "{page}");
    let unreadable = described(with_members(r#""manufacturer":"a string""#)?).await?;
    assert!(
        unreadable.contains("Declared in a form the console cannot read"),
        "{unreadable}"
    );
    Ok(())
}

// A value the gateway declares is escaped in the cards and in the document,
// where the page writes it.
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
        page.contains("&#60;script&#62;alert(1)&#60;/script&#62;"),
        "{page}"
    );
    assert!(page.contains("&#60;img src=x&#62;"), "{page}");
    Ok(())
}

// The OPTIONS document has its one home on About: highlighted on the page,
// and downloaded whole as an attachment by a signed-in operator alone.
#[tokio::test]
async fn about_carries_the_options_document_and_its_download() -> Result<(), Box<dyn Error>> {
    let gateway = MockServer::start().await;
    Mock::given(method("OPTIONS"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(options_example()?),
        )
        .mount(&gateway)
        .await;
    let (state, service) = console(&format!(
        "[gateway]\nbase_url = \"{}\"\n\n[session]\nsecure_cookie = false\n",
        gateway.uri()
    ))?;
    let session = state.sessions().establish(signed_in())?;
    let (_response, page) = send(&service, get_as("/about", &session)?).await?;
    assert!(
        page.contains(r#"<span class="fui-tok-key">&quot;spec_version&quot;</span>"#),
        "{page}"
    );
    assert!(page.contains(r#"href="/about/options.json""#), "{page}");
    let (response, document) = send(&service, get_as("/about/options.json", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{document}");
    assert_eq!(
        "attachment; filename=\"options.json\"",
        crate::support::header(&response, "content-disposition")
    );
    let read: openehr_federation::options::OptionsRoot = serde_json::from_str(&document)?;
    let example: openehr_federation::options::OptionsRoot =
        serde_json::from_str(&options_example()?)?;
    assert_eq!(example.federation.id, read.federation.id, "{document}");
    let (response, _) = send(&service, crate::support::get("/about/options.json")?).await?;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    Ok(())
}
