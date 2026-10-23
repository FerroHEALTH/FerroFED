// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The About page, which names the manufacturer in full.
//!
//! It shows the product, its version, the gateway the console reads, and
//! the manufacturer, which Regulation (EU) 2025/327 Art 30(1)(g) asks an EHR
//! system to carry. Every page's footer links to it, and it needs no
//! sign-in.
//!
//! The page is data: each card is a title and its rows, drawn by one
//! function, so the browser bundle carries the drawing once.

use ferrohealth_ui::view::surface::{card, definitions, grid_start, page_header};
use leptos::either::EitherOf4;
use leptos::prelude::*;
use leptos_meta::Title;

use crate::app::{PRODUCT, VERSION};
use crate::locale::{Msg, tr};
use crate::manufacturer::MANUFACTURER;
use crate::views::titled;

/// The value of one row of a card.
enum Value {
    /// Plain text.
    Text(String),
    /// A link to the address `href`, showing `text`.
    Link {
        /// Where it leads.
        href: String,
        /// What it says.
        text: String,
    },
    /// A value in the monospace stack.
    Code(String),
    /// A line saying what is not shown, and why.
    Missing(String),
}

/// The view of `value`.
fn value(value: Value) -> impl IntoView {
    match value {
        Value::Text(text) => EitherOf4::A(text),
        Value::Link { href, text } => EitherOf4::B(view! { <a href=href>{text}</a> }),
        Value::Code(text) => EitherOf4::C(view! { <code>{text}</code> }),
        Value::Missing(text) => EitherOf4::D(view! { <span class="fui-muted">{text}</span> }),
    }
}

/// A card titled `title` listing `rows`, each a term and its value.
fn facts(title: String, rows: Vec<(String, Value)>) -> AnyView {
    let rows = rows
        .into_iter()
        .map(|(term, shown)| (term, value(shown).into_any()))
        .collect();
    card(title, definitions(rows)).into_any()
}

/// The cards of the page: the product with the gateway's `{base}`, which
/// `gateway` holds for a signed-in operator alone, and the manufacturer
/// (Art 30(1)(g): the name, the postal address and the single point of
/// contact).
fn cards(gateway: String) -> Vec<(String, Vec<(String, Value)>)> {
    let gateway = if gateway.is_empty() {
        Value::Missing(tr(Msg::SignSeeGatewayConsoleReads))
    } else {
        Value::Code(gateway)
    };
    vec![
        (
            tr(Msg::Product),
            vec![
                (tr(Msg::Product), Value::Text(PRODUCT.to_owned())),
                (tr(Msg::Version), Value::Text(VERSION.to_owned())),
                (tr(Msg::Purpose), Value::Text(tr(Msg::AboutPurpose))),
                (tr(Msg::Gateway), gateway),
            ],
        ),
        (
            tr(Msg::Manufacturer),
            vec![
                (tr(Msg::Name), Value::Text(MANUFACTURER.name.to_owned())),
                (
                    tr(Msg::PostalAddress),
                    Value::Text(MANUFACTURER.postal_address.to_owned()),
                ),
                (
                    tr(Msg::Contact),
                    Value::Link {
                        href: format!("mailto:{}", MANUFACTURER.email),
                        text: MANUFACTURER.email.to_owned(),
                    },
                ),
                (
                    tr(Msg::Website),
                    Value::Link {
                        href: MANUFACTURER.website.to_owned(),
                        text: MANUFACTURER.website.to_owned(),
                    },
                ),
            ],
        ),
    ]
}

/// The About page; `gateway` is the gateway's `{base}` for a signed-in
/// operator and empty for a visitor.
///
/// The page is static, so the server renders its markup and the page shows
/// that HTML ([`crate::home`]); every link to it is a full load.
#[component]
#[expect(
    clippy::must_use_candidate,
    reason = "the component macro writes the function it returns without the attributes on the one written here (https://docs.rs/leptos/0.8/leptos/attr.component.html)"
)]
pub fn AboutPage(
    /// The gateway's `{base}`, or empty when it is not shown.
    gateway: String,
) -> impl IntoView {
    let html =
        SharedValue::new_str(move || crate::home::rendered(move || about(gateway))).into_inner();
    view! {
        <Title text=titled(&tr(Msg::About)) />
        <div inner_html=html></div>
    }
}

/// The markup of the About page, naming `gateway` when it is not empty.
#[must_use]
pub fn about(gateway: String) -> String {
    let cards = cards(gateway)
        .into_iter()
        .map(|(title, rows)| facts(title, rows))
        .collect_view();
    view! {
        {page_header(tr(Msg::About), Some(tr(Msg::ProductGatewayReadsWhoMakes)), ())}
        {grid_start(cards)}
    }
    .to_html()
}
