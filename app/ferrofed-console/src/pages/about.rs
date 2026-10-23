// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! About: the console, the gateway it reads, and the manufacturer, which
//! Regulation (EU) 2025/327 Art 30(1)(g) asks an EHR system to carry with its
//! name, postal address and point of contact.
//!
//! Under the two cards a signed-in operator reads the gateway's `OPTIONS
//! {base}/` document, its one home, with Copy and Download (FerroUI's About
//! template). A visitor reads the cards in the centred layout, with no
//! document.

use ferrohealth_ui::highlight::Language;
use ferrohealth_ui::view::replica::Replica;
use ferrohealth_ui::view::shell::centred as centred_layout;
use ferrohealth_ui::view::surface::{Header, card, definitions};
use ferrohealth_ui::view::template::about::{
    AboutPage, Link, Manufacturer, SelfDescription, about_page,
};
use leptos::prelude::*;

use crate::app::{PRODUCT, VERSION};
use crate::locale::{Msg, tr, tr_with};
use crate::manufacturer::MANUFACTURER;

/// The route Download fetches the `OPTIONS {base}/` document from.
pub const OPTIONS_DOCUMENT: &str = "/about/options.json";

/// The id of the document's Copy button.
const COPY_ID: &str = "options-copy";

/// The FerroUI version the console is built on, as its manifest pins it.
const FERROUI: &str = "0.3.2";

/// What the gateway told the console of itself, for the Versions card and
/// the document.
struct Gateway {
    /// The gateway's address, for a signed-in operator.
    address: Option<String>,
    version: Option<String>,
    spec: Option<String>,
    replica: Option<String>,
    document: Option<String>,
}

/// Reads what the About page shows of the gateway; nothing for a visitor
/// or a gateway that does not answer.
async fn gateway() -> Gateway {
    let deployment = crate::read::deployment().await.ok();
    let (address, description) = match crate::read::signed_in() {
        Ok((state, token)) => (
            Some(state.settings().gateway.base.to_string()),
            state.gateway().self_description(&token).await.ok(),
        ),
        Err(_) => (None, None),
    };
    Gateway {
        address,
        version: deployment
            .as_ref()
            .map(|deployment| deployment.version.clone()),
        replica: deployment.and_then(|deployment| deployment.replica),
        spec: description
            .as_ref()
            .map(|description| description.federation.spec_version.to_string()),
        document: description
            .as_ref()
            .and_then(|description| serde_json::to_string_pretty(description).ok()),
    }
}

/// The page's cards for `gateway`.
fn about(gateway: Gateway) -> AnyView {
    let text = |words: String| words.into_any();
    let signed_in = gateway.address.is_some();
    let mut versions = vec![(
        tr(Msg::AboutGateway),
        text(gateway.version.map_or_else(
            || {
                tr(if signed_in {
                    Msg::AboutGatewayUnanswered
                } else {
                    Msg::AboutGatewayUnknown
                })
            },
            |version| tr_with(Msg::AboutGatewayVersion, &[("version", version.into())]),
        )),
    )];
    if let Some(address) = gateway.address {
        versions.push((
            tr(Msg::AboutGatewayAddress),
            view! { <code>{address}</code> }.into_any(),
        ));
    }
    if let Some(spec) = gateway.spec {
        versions.push((
            tr(Msg::AboutSpecification),
            text(tr_with(
                Msg::AboutSpecificationValue,
                &[("version", spec.into())],
            )),
        ));
    }
    versions.push((tr(Msg::AboutOpenehr), text(tr(Msg::AboutOpenehrValue))));
    versions.push((
        tr(Msg::AboutFerroui),
        text(tr_with(
            Msg::AboutFerrouiValue,
            &[("version", FERROUI.into())],
        )),
    ));
    let address = card(
        tr(Msg::AboutContactTitle),
        definitions(vec![
            (tr(Msg::AboutName), MANUFACTURER.name.into_any()),
            (
                tr(Msg::AboutAddress),
                MANUFACTURER.postal_address.into_any(),
            ),
            (
                tr(Msg::AboutWebsite),
                view! { <a href=MANUFACTURER.website>{MANUFACTURER.website}</a> }.into_any(),
            ),
        ]),
    )
    .into_any();
    about_page(AboutPage {
        header: Header {
            title: tr(Msg::AboutTitle).into(),
            subtitle: Some(tr(Msg::AboutSubtitle).into()),
            ..Header::default()
        },
        release: format!("{PRODUCT} {VERSION}"),
        versions,
        manufacturer: Manufacturer {
            licence: tr(Msg::AboutLicence),
            support: Link {
                label: MANUFACTURER.email.to_owned(),
                href: format!("mailto:{}", MANUFACTURER.email),
            },
            documentation: Some(Link {
                label: tr(Msg::AboutBook),
                href: String::from("https://ferrofed.eu/docs/operate/operator-console.html"),
            }),
        },
        cards: vec![address],
        self_description: gateway.document.map(|text| SelfDescription {
            title: tr(Msg::AboutDocumentTitle),
            line: tr(Msg::AboutDocumentLine),
            language: Language::Json,
            text,
            copy_id: String::from(COPY_ID),
            download: String::from(OPTIONS_DOCUMENT),
        }),
        replica: gateway.replica.map(|name| Replica { name, at: None }),
    })
}

/// About, in the shell of a signed-in operator.
#[must_use]
pub fn page() -> AnyView {
    let body = Suspend::new(async move { about(gateway().await) });
    view! {
        {crate::app::title(tr(Msg::AboutTitle))}
        <Suspense fallback=|| ()>{body}</Suspense>
    }
    .into_any()
}

/// About, in the centred layout a visitor reads.
#[must_use]
pub fn centred() -> AnyView {
    let body = about(Gateway {
        address: None,
        version: None,
        spec: None,
        replica: None,
        document: None,
    });
    view! {
        {crate::app::title(tr(Msg::AboutTitle))}
        {centred_layout(
            view! { <div class="fui-column">{body}</div> }.into_any(),
            crate::shell::footer(),
        )}
    }
    .into_any()
}
