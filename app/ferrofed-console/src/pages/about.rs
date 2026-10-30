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
use ferrohealth_ui::html::Html;
use ferrohealth_ui::view::replica::Replica;
use ferrohealth_ui::view::shell::centred as centred_layout;
use ferrohealth_ui::view::surface::{Header, card, definitions};
use ferrohealth_ui::view::template::about::{
    AboutPage, Link, Manufacturer, SelfDescription, about_page,
};
use ferrohealth_ui::view::toast::toast_region;

use crate::app::{PRODUCT, VERSION};
use crate::locale::{Msg, current, tr, tr_with};
use crate::pages::{h, t};
use ferrofed_registry::manufacturer::MANUFACTURER;

/// The route Download fetches the `OPTIONS {base}/` document from.
pub const OPTIONS_DOCUMENT: &str = "/about/options.json";

/// The id of the document's Copy button.
const COPY_ID: &str = "options-copy";

/// The FerroUI version the console is built on, as its manifest pins it.
const FERROUI: &str = "0.6.0";

/// What the gateway told the console of itself, for the Versions card and
/// the document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Gateway {
    /// The gateway's address, for a signed-in operator.
    address: Option<String>,
    /// The gateway's product version.
    version: Option<String>,
    /// The specification version it declares.
    spec: Option<String>,
    /// The replica that answered.
    replica: Option<String>,
    /// Its `OPTIONS {base}/` document, pretty-printed.
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
fn about(gateway: Gateway) -> Html {
    let text = |words: String| t(words);
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
            h(format!("<code>{}</code>", t(address))),
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
            (tr(Msg::AboutName), t(MANUFACTURER.name)),
            (tr(Msg::AboutAddress), t(MANUFACTURER.postal_address)),
            (
                tr(Msg::AboutWebsite),
                h(format!(
                    "<a href=\"{website}\">{website}</a>",
                    website = t(MANUFACTURER.website)
                )),
            ),
        ]),
    );
    about_page(
        &current(),
        AboutPage {
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
        },
    )
}

/// About, in the shell of a signed-in operator.
pub async fn page() -> Html {
    let gateway = gateway().await;
    crate::app::title(tr(Msg::AboutTitle)).then(about(gateway))
}

/// About, in the centred layout a visitor reads.
#[must_use]
pub fn centred() -> Html {
    let body = about(Gateway::default());
    let locale = current();
    crate::app::title(tr(Msg::AboutTitle)).then(centred_layout(
        &locale,
        h(format!("<div class=\"fui-column\">{body}</div>")),
        toast_region(&locale, Vec::new()),
        crate::shell::footer(),
    ))
}
