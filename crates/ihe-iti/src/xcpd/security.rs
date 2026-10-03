// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The SAML 2.0 assertion of Cross-Enterprise User Assertion (XUA, ITI-40)
//! that a deployment supplies for an ITI-55 request.
//!
//! The crate signs nothing. A deployment's identity provider or security
//! token service issues and signs the assertion, and the client sends its
//! bytes unchanged inside a WS-Security `Security` header, so its XML
//! signature still verifies at the responding gateway. Whether a network
//! requires an assertion at all is the network's policy: ITI-55 itself
//! requires none (ITI TF-1 Table 27.1.3-1 groups the actors with ATNA and
//! CT only, and §27.4.2 leaves policy to each community).

use std::fmt;
use std::sync::Arc;

use quick_xml::NsReader;
use quick_xml::events::Event;
use quick_xml::name::ResolveResult;

use super::error::InvalidInput;
use crate::redact::REDACTED;

/// The SAML 2.0 assertion namespace (OASIS SAML 2.0 Core §2.3.3).
const SAML2: &str = "urn:oasis:names:tc:SAML:2.0:assertion";

/// A signed SAML 2.0 assertion, held as the bytes of its one `Assertion`
/// element.
///
/// It is a bearer credential and may name the user: `Debug` shows nothing
/// of it, and there is no `Display`.
#[derive(Clone)]
pub struct XuaAssertion(Arc<str>);

impl XuaAssertion {
    /// Reads an assertion from `xml`: one `saml2:Assertion` element, after an
    /// optional XML declaration, comments and white space, and before
    /// trailing comments and white space only.
    ///
    /// The element's bytes are kept exactly as written; the declaration and
    /// anything around the element are dropped, since the element is
    /// embedded inside the SOAP header.
    ///
    /// # Errors
    /// [`InvalidInput::Assertion`] when `xml` is not well formed, carries a
    /// document type declaration, or holds anything but one SAML 2.0
    /// `Assertion` element.
    pub fn new(xml: &str) -> Result<Self, InvalidInput> {
        let (start, end) = element_span(xml).ok_or(InvalidInput::Assertion)?;
        let element = xml.get(start..end).ok_or(InvalidInput::Assertion)?;
        Ok(Self(Arc::from(element)))
    }

    /// The `Assertion` element, as written.
    pub(super) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for XuaAssertion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("XuaAssertion").field(&REDACTED).finish()
    }
}

/// The byte span of the one SAML 2.0 `Assertion` element `xml` holds, or
/// `None` when it holds anything else.
fn element_span(xml: &str) -> Option<(usize, usize)> {
    let mut reader = NsReader::from_str(xml);
    let mut start = None;
    let mut end = None;
    let mut depth = 0_usize;
    loop {
        let before = usize::try_from(reader.buffer_position()).ok()?;
        let (namespace, event) = reader.read_resolved_event().ok()?;
        let saml = matches!(namespace, ResolveResult::Bound(ns) if ns.0 == SAML2);
        match event {
            Event::Start(element) => {
                if depth == 0 {
                    let root =
                        end.is_none() && saml && element.local_name().as_ref() == "Assertion";
                    if !root {
                        return None;
                    }
                    start = Some(before);
                }
                depth = depth.checked_add(1)?;
            }
            Event::End(_) => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    end = Some(usize::try_from(reader.buffer_position()).ok()?);
                }
            }
            Event::Empty(element) if depth == 0 => {
                let root = end.is_none() && saml && element.local_name().as_ref() == "Assertion";
                if !root {
                    return None;
                }
                start = Some(before);
                end = Some(usize::try_from(reader.buffer_position()).ok()?);
            }
            Event::Text(text) if depth == 0 => {
                if !text.xml10_content().trim().is_empty() {
                    return None;
                }
            }
            Event::Decl(_) if start.is_none() && before == 0 => {}
            Event::DocType(_) | Event::Decl(_) => return None,
            Event::Eof => break,
            _ if depth == 0 && !matches!(event, Event::Comment(_)) => return None,
            _ => {}
        }
    }
    Some((start?, end?))
}

#[cfg(test)]
mod tests {
    use super::XuaAssertion;

    const ASSERTION: &str = r#"<saml2:Assertion xmlns:saml2="urn:oasis:names:tc:SAML:2.0:assertion" ID="_a" Version="2.0"><saml2:Issuer>urn:oid:2.999.7</saml2:Issuer></saml2:Assertion>"#;

    #[test]
    fn the_element_is_kept_byte_for_byte_and_the_declaration_dropped() {
        let written =
            format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- issued -->\n{ASSERTION}\n");
        let read = XuaAssertion::new(&written).map(|assertion| assertion.as_str().to_owned());
        assert_eq!(Ok(ASSERTION.to_owned()), read);
    }

    #[test]
    fn anything_but_one_saml_assertion_is_refused() {
        for written in [
            "",
            "not xml",
            r#"<Assertion xmlns="urn:example"/>"#,
            &format!("{ASSERTION}{ASSERTION}"),
            &format!("{ASSERTION}trailing"),
            &format!("<!DOCTYPE a>{ASSERTION}"),
            r#"<saml2:Assertion xmlns:saml2="urn:oasis:names:tc:SAML:2.0:assertion">"#,
        ] {
            assert!(XuaAssertion::new(written).is_err(), "refused: {written:?}");
        }
    }

    #[test]
    fn debug_shows_nothing_of_it() {
        let rendered = XuaAssertion::new(ASSERTION).map(|assertion| format!("{assertion:?}"));
        assert_eq!(Ok("XuaAssertion(\"***\")".to_owned()), rendered);
    }
}
