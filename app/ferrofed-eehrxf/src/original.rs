// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The checks a member's own document passes before the FHIR face serves it
//! (MyHealth@EU Requirements Catalogue 10.0.0 §1.16 and §1.16.1).
//!
//! [`header`] holds the `DocumentReference` a member serves itself to the
//! MyHealth@EU `NCPeH` API profile `DocumentReference-minimal-myhealtheu`
//! (`myhealth.eu.fhir.ncp-api` 9.1.1-ci), the header data a contact point
//! reads, and asks every attachment to name its media type, which the read
//! is checked by. [`original`] holds the bytes the member answers the read
//! with to that media type: a FHIR document passes as it is, and an original
//! is admitted only in a format the exchange takes: a PDF that declares
//! PDF/A-1a or PDF/A-1b (draft `EEHRxF` implementing act Art 3(1)(a) and
//! Annex II), a JPEG or PNG image, and for imaging a DICOM file (Art
//! 3(1)(d)), each by the signature its bytes open with. No refusal quotes a
//! value of the document.

use std::io::Cursor;
use std::sync::LazyLock;

use eehrxf::category::Category;
use eehrxf::dataset::{DatasetError, ResourceProfile};
use eehrxf::document::representation::{PdfA, RepresentationError};
use eehrxf::receive::conform::{CheckError, check_resource};
use fhir_types::codec::Value;

/// The canonical URL of the MyHealth@EU `DocumentReference` profile a
/// member's own document is described by.
pub const DOCUMENT_REFERENCE_MINIMAL: &str =
    "http://fhir.ehdsi.eu/ncp-api/StructureDefinition/DocumentReference-minimal-myhealtheu";

/// The media type of a PDF document (RFC 8118).
pub const PDF: &str = "application/pdf";

/// The media type of a JPEG image (RFC 2046 §4.2).
pub const JPEG: &str = "image/jpeg";

/// The media type of a PNG image (RFC 2083).
pub const PNG: &str = "image/png";

/// The media type of a DICOM file (RFC 3240).
pub const DICOM: &str = "application/dicom";

/// The profile [`header`] holds a document to, read once from the vendored
/// package.
static MINIMAL: LazyLock<Result<ResourceProfile, DatasetError>> = LazyLock::new(|| {
    ResourceProfile::read(
        Cursor::new(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/specs/ehdsi/myhealth.eu.fhir.ncp-api-9.1.1-ci.tgz"
        ))),
        DOCUMENT_REFERENCE_MINIMAL,
    )
});

/// Why a member's `DocumentReference` is not listed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Unheaded {
    /// The profile cannot be read from the package this build carries.
    #[error("the profile {DOCUMENT_REFERENCE_MINIMAL} cannot be read")]
    Profile(#[source] &'static DatasetError),
    /// The resource is no JSON object.
    #[error("the DocumentReference is no JSON object")]
    NotObject,
    /// The resource breaks the profile.
    #[error("the DocumentReference breaks DocumentReference-minimal-myhealtheu: {0}")]
    Profiled(#[source] CheckError),
    /// An attachment names no media type.
    #[error("content[{0}].attachment names no contentType")]
    NoMediaType(usize),
}

/// Holds `document`, a `DocumentReference` a member serves itself, to
/// `DocumentReference-minimal-myhealtheu`, with a media type on every
/// attachment.
///
/// A `required` binding the profile names is not evaluated: the package
/// carries no expansion of the value sets it binds.
///
/// # Errors
///
/// The [`Unheaded`] that says why the document is not listed.
pub fn header(document: &Value) -> Result<(), Unheaded> {
    let profile = MINIMAL.as_ref().map_err(Unheaded::Profile)?;
    let Value::Object(resource) = document else {
        return Err(Unheaded::NotObject);
    };
    check_resource(profile, "DocumentReference", resource).map_err(Unheaded::Profiled)?;
    // NOTE: FHIR R4 Attachment.contentType: the media type is what the bytes are read by, so the
    // read can hold the original to its format (MyHealth@EU Requirements Catalogue 10.0.0 §1.16.1).
    let contents = resource
        .get("content")
        .and_then(Value::as_array)
        .unwrap_or_default();
    for (position, content) in contents.iter().enumerate() {
        let named = content
            .get("attachment")
            .and_then(|attachment| attachment.get("contentType"))
            .and_then(Value::as_str)
            .is_some_and(|media| !media.trim().is_empty());
        if !named {
            return Err(Unheaded::NoMediaType(position));
        }
    }
    Ok(())
}

/// What the bytes of a read are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Original {
    /// A FHIR resource, which is no original-format document.
    Fhir,
    /// A PDF/A-1 document.
    PdfA,
    /// A JPEG image.
    Jpeg,
    /// A PNG image.
    Png,
    /// A DICOM file.
    Dicom,
}

/// Why the bytes of a member's document are not served.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Unoriginal {
    /// The member named no media type.
    #[error("the member named no media type")]
    NoMediaType,
    /// The media type is none the exchange takes for the category.
    #[error("{media} is no format the exchange takes for {category}")]
    Format {
        /// The media type, without parameters.
        media: String,
        /// The category's code.
        category: &'static str,
    },
    /// The PDF is no PDF/A-1a or PDF/A-1b document.
    #[error("the PDF is no PDF/A-1 document")]
    NotPdfA(#[source] RepresentationError),
    /// The bytes do not open with the signature of their media type.
    #[error("the bytes are no {media} file")]
    Signature {
        /// The media type the member named.
        media: &'static str,
    },
}

/// Holds `bytes`, the answer to the read of a document of `category` a
/// member serves itself, to the media type `media` the member named.
///
/// # Errors
///
/// The [`Unoriginal`] that says why the bytes are not served.
pub fn original(
    media: Option<&str>,
    bytes: &[u8],
    category: Category,
) -> Result<Original, Unoriginal> {
    let media = media
        .and_then(|media| media.split(';').next())
        .map(|media| media.trim().to_ascii_lowercase())
        .filter(|media| !media.is_empty())
        .ok_or(Unoriginal::NoMediaType)?;
    match media.as_str() {
        // NOTE: FHIR R4 http.html §3.1.0.1.10: the FHIR media types; the answer is a resource,
        // which the structured representation of draft EEHRxF Art 3(1)(c) carries.
        "application/fhir+json"
        | "application/fhir+xml"
        | "application/json+fhir"
        | "application/xml+fhir" => Ok(Original::Fhir),
        PDF => PdfA::new(bytes.to_vec())
            .map(|_| Original::PdfA)
            .map_err(Unoriginal::NotPdfA),
        // NOTE: ISO/IEC 10918-1 Annex B.1.1.3: a JPEG stream opens with the SOI marker FF D8 and the
        // next marker's FF.
        JPEG => signed(bytes.starts_with(&[0xFF, 0xD8, 0xFF]), JPEG, Original::Jpeg),
        // NOTE: RFC 2083 §3.1: the eight-byte PNG signature.
        PNG => signed(
            bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]),
            PNG,
            Original::Png,
        ),
        // NOTE: DICOM PS3.10 §7.1: a 128-byte preamble, then the prefix "DICM"; draft EEHRxF
        // Art 3(1)(d) admits DICOM for imaging alone.
        DICOM if category == Category::Imaging => signed(
            bytes.get(128..132) == Some(b"DICM".as_slice()),
            DICOM,
            Original::Dicom,
        ),
        _ => Err(Unoriginal::Format {
            media,
            category: category.code(),
        }),
    }
}

/// `original` when the signature was `found`, the refusal naming `media`
/// otherwise.
fn signed(found: bool, media: &'static str, original: Original) -> Result<Original, Unoriginal> {
    if found {
        Ok(original)
    } else {
        Err(Unoriginal::Signature { media })
    }
}

#[cfg(test)]
mod tests {
    use eehrxf::category::Category;
    use fhir_types::codec::Value;

    use super::{Original, Unheaded, Unoriginal, header, original};

    /// A `DocumentReference` carrying every element the profile requires.
    const HEADED: &str = r#"{"resourceType":"DocumentReference","masterIdentifier":{"system":"urn:ietf:rfc:3986","value":"urn:uuid:1ab00000-0000-4000-8000-0000000000a1"},"status":"current","type":{"coding":[{"system":"http://loinc.org","code":"11502-2"}]},"subject":{"reference":"Patient/pat-a"},"date":"2026-11-01T10:00:00Z","description":"Synthetic laboratory report","content":[{"attachment":{"contentType":"application/pdf","url":"Binary/lab-1"}}]}"#;

    /// A minimal PDF/A-1b, its identification schema in plain XMP.
    const PDF_A: &[u8] = b"%PDF-1.4\n<x:xmpmeta><rdf:Description pdfaid:part=\"1\" pdfaid:conformance=\"B\"/></x:xmpmeta>\n%%EOF\n";

    fn parsed(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn a_document_reference_with_every_required_element_is_listed() {
        header(&parsed(HEADED)).unwrap();
    }

    #[test]
    fn a_document_reference_with_no_description_breaks_the_profile() {
        let unheaded = header(&parsed(
            &HEADED.replace(r#""description":"Synthetic laboratory report","#, ""),
        ))
        .unwrap_err();
        assert!(matches!(unheaded, Unheaded::Profiled(_)), "{unheaded}");
        assert!(unheaded.to_string().contains("description"), "{unheaded}");
    }

    #[test]
    fn an_attachment_with_no_media_type_is_refused() {
        let unheaded = header(&parsed(
            &HEADED.replace(r#""contentType":"application/pdf","#, ""),
        ))
        .unwrap_err();
        assert!(matches!(unheaded, Unheaded::NoMediaType(0)), "{unheaded}");
    }

    #[test]
    fn a_pdf_a_1_is_admitted_and_a_plain_pdf_is_not() {
        assert_eq!(
            Original::PdfA,
            original(Some("application/pdf"), PDF_A, Category::Laboratory).unwrap()
        );
        let plain = original(
            Some("application/pdf"),
            b"%PDF-1.7\n%%EOF\n",
            Category::Laboratory,
        )
        .unwrap_err();
        assert!(matches!(plain, Unoriginal::NotPdfA(_)), "{plain}");
    }

    #[test]
    fn an_image_is_held_to_its_signature() {
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0];
        assert_eq!(
            Original::Png,
            original(Some("image/png"), &png, Category::Imaging).unwrap()
        );
        assert_eq!(
            Original::Jpeg,
            original(
                Some("image/jpeg"),
                &[0xFF, 0xD8, 0xFF, 0xE0],
                Category::Discharge
            )
            .unwrap()
        );
        let wrong = original(Some("image/jpeg"), &png, Category::Imaging).unwrap_err();
        assert!(
            matches!(
                wrong,
                Unoriginal::Signature {
                    media: "image/jpeg"
                }
            ),
            "{wrong}"
        );
    }

    #[test]
    fn dicom_is_admitted_for_imaging_alone() {
        let mut dicom = vec![0_u8; 128];
        dicom.extend_from_slice(b"DICM");
        assert_eq!(
            Original::Dicom,
            original(Some("application/dicom"), &dicom, Category::Imaging).unwrap()
        );
        let refused =
            original(Some("application/dicom"), &dicom, Category::Laboratory).unwrap_err();
        assert!(matches!(refused, Unoriginal::Format { .. }), "{refused}");
    }

    #[test]
    fn a_fhir_answer_passes_and_another_format_is_refused() {
        assert_eq!(
            Original::Fhir,
            original(
                Some("application/fhir+json; charset=utf-8"),
                b"{}",
                Category::Laboratory
            )
            .unwrap()
        );
        let text = original(Some("text/plain"), b"report", Category::Laboratory).unwrap_err();
        assert!(matches!(text, Unoriginal::Format { .. }), "{text}");
        let none = original(None, b"report", Category::Laboratory).unwrap_err();
        assert!(matches!(none, Unoriginal::NoMediaType), "{none}");
    }
}
