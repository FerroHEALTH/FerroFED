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

use leptos::either::EitherOf4;
use leptos::prelude::*;
use leptos_meta::Title;

use crate::app::{PRODUCT, VERSION};
use crate::components::{card, page_header};
use crate::manufacturer::MANUFACTURER;
use crate::views::titled;

/// The value of one row of a card.
enum Value {
    /// Plain text.
    Text(&'static str),
    /// A link to the address `href`, showing `text`.
    Link {
        /// Where it leads.
        href: String,
        /// What it says.
        text: &'static str,
    },
    /// A value in the monospace stack.
    Code(String),
    /// A line saying what is not shown, and why.
    Missing(&'static str),
}

/// The view of `value`.
fn value(value: Value) -> impl IntoView {
    match value {
        Value::Text(text) => EitherOf4::A(text),
        Value::Link { href, text } => EitherOf4::B(view! { <a href=href>{text}</a> }),
        Value::Code(text) => EitherOf4::C(view! { <code>{text}</code> }),
        Value::Missing(text) => EitherOf4::D(view! { <span class="text-muted">{text}</span> }),
    }
}

/// A card titled `title` listing `rows`, each a term and its value.
fn facts(title: &'static str, rows: Vec<(&'static str, Value)>) -> AnyView {
    let rows = rows
        .into_iter()
        .map(|(term, shown)| {
            view! {
                <div class="grid grid-cols-1 gap-1 border-b border-line py-2 last:border-b-0 sm:grid-cols-[10rem_1fr]">
                    <dt class="text-sm font-medium text-muted">{term}</dt>
                    <dd class="m-0 text-sm break-words">{value(shown)}</dd>
                </div>
            }
        })
        .collect_view();
    card(title, view! { <dl class="m-0">{rows}</dl> }).into_any()
}

/// The cards of the page: the product, the manufacturer (Art 30(1)(g): the
/// name, the postal address and the single point of contact), and the
/// gateway's `{base}`, which `gateway` holds for a signed-in operator alone.
fn cards(gateway: String) -> Vec<(&'static str, Vec<(&'static str, Value)>)> {
    let gateway = if gateway.is_empty() {
        Value::Missing("Sign in to see the gateway this console reads.")
    } else {
        Value::Code(gateway)
    };
    vec![
        (
            "Product",
            vec![
                ("Product", Value::Text(PRODUCT)),
                ("Version", Value::Text(VERSION)),
                (
                    "Purpose",
                    Value::Text(
                        "The operator console of FerroFED, the openEHR federation gateway. \
                         It holds no clinical data.",
                    ),
                ),
            ],
        ),
        (
            "Manufacturer",
            vec![
                ("Name", Value::Text(MANUFACTURER.name)),
                ("Postal address", Value::Text(MANUFACTURER.postal_address)),
                (
                    "Contact",
                    Value::Link {
                        href: format!("mailto:{}", MANUFACTURER.email),
                        text: MANUFACTURER.email,
                    },
                ),
                (
                    "Website",
                    Value::Link {
                        href: MANUFACTURER.website.to_owned(),
                        text: MANUFACTURER.website,
                    },
                ),
            ],
        ),
        ("Gateway", vec![("Base URL", gateway)]),
    ]
}

/// The About page; `gateway` is the gateway's `{base}` for a signed-in
/// operator and empty for a visitor.
#[component]
#[expect(
    clippy::must_use_candidate,
    reason = "the component macro writes the function it returns without the attributes on the one written here (https://docs.rs/leptos/0.8/leptos/attr.component.html)"
)]
pub fn AboutPage(
    /// The gateway's `{base}`, or empty when it is not shown.
    gateway: String,
) -> impl IntoView {
    let cards = cards(gateway)
        .into_iter()
        .map(|(title, rows)| facts(title, rows))
        .collect_view();
    view! {
        <Title text=titled("About") />
        <div class="mx-auto w-full max-w-3xl">
            {page_header("About", "The product, the gateway it reads, and who makes it.", ())}
            <div class="flex flex-col gap-4">{cards}</div>
        </div>
    }
}
