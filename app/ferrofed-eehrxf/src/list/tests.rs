// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

use super::{ListRefusal, ListRequest, Match, Subject, attachment_urls, readdressed, searchset};
use crate::list::criteria::CriterionError;
use fhir_search::date::DateError;
use fhir_search::token::TokenError;
use fhir_types::codec::Value;

#[test]
fn the_summary_written_on_request_is_listed_as_an_on_demand_entry() {
    let listing = super::Listing {
        system: "urn:oid:2.999.1",
        value: "synthetic-1",
        patient: "patient-id",
        date: "2026-11-25T10:00:00Z",
    };
    let operator = crate::summary::Organisation {
        id: String::from("operator"),
        name: None,
        identifiers: Vec::new(),
    };
    let entry = super::on_demand(("entry-id", "urn:uuid:entry"), listing, &operator);
    let [content] = entry.content.as_slice() else {
        panic!("one content");
    };
    // MHD 4.2.2 ex-DocumentReferenceComprehensiveOnDemand: an On-Demand entry carries
    // neither hash nor size, which only a written document has.
    assert!(content.attachment.hash.is_none() && content.attachment.size.is_none());
    assert!(content.attachment.url.is_some());
}

#[test]
fn the_patient_and_the_summary_type_are_read() {
    for query in [
        "patient.identifier=urn%3Aoid%3A2.999.1%7Csynthetic-1",
        "patient.identifier=urn%3Aoid%3A2.999.1%7Csynthetic-1&type=http%3A%2F%2Floinc.org%7C60591-5",
        "patient.identifier=urn%3Aoid%3A2.999.1%7Csynthetic-1&type=60591-5",
        "patient.identifier=urn%3Aoid%3A2.999.1%7Csynthetic-1&type=http%3A%2F%2Floinc.org%7C",
        "patient.identifier=urn%3Aoid%3A2.999.1%7Csynthetic-1&type=x%7Cy,60591-5&_format=json",
    ] {
        let read = ListRequest::from_query(Some(query)).unwrap();
        assert_eq!(
            read.subject(),
            &Subject::Identifier {
                system: "urn:oid:2.999.1".to_owned(),
                value: "synthetic-1".to_owned()
            },
            "{query}"
        );
        assert!(read.admits_summaries(), "{query}");
    }
}

#[test]
fn one_id_alone_names_its_document_and_more_name_no_patient() {
    let read = ListRequest::from_query(Some("_id=abc")).unwrap();
    assert_eq!(read.subject(), &Subject::Document("abc".to_owned()));
    let read = ListRequest::from_query(Some("_id=abc&patient.identifier=a%7Cb")).unwrap();
    assert!(matches!(read.subject(), Subject::Identifier { .. }));
    for query in ["_id=a,b", "_id=a&_id=b"] {
        assert_eq!(
            ListRequest::from_query(Some(query)),
            Err(ListRefusal::NoPatient),
            "{query}"
        );
    }
    assert_eq!(
        ListRequest::read("abc").unwrap().subject(),
        &Subject::Document("abc".to_owned())
    );
}

#[test]
fn a_patient_reference_is_read_as_sent() {
    let read = ListRequest::from_query(Some("patient=Patient%2Fabc&type=60591-5")).unwrap();
    assert_eq!(read.subject(), &Subject::Patient("Patient/abc".to_owned()));
    assert!(!format!("{read:?}").contains("abc"));
}

#[test]
fn another_type_admits_no_summary() {
    for query in [
        "patient.identifier=a%7Cb&type=http%3A%2F%2Floinc.org%7C11502-2",
        "patient.identifier=a%7Cb&type=%7C60591-5",
        "patient.identifier=a%7Cb&type=http%3A%2F%2Fexample.org%7C60591-5",
        "patient.identifier=a%7Cb&type=60591-5&type=11502-2",
    ] {
        let read = ListRequest::from_query(Some(query)).unwrap();
        assert!(!read.admits_summaries(), "{query}");
    }
}

#[test]
fn every_other_form_is_refused() {
    for (query, refusal) in [
        ("", ListRefusal::NoPatient),
        ("type=60591-5", ListRefusal::NoPatient),
        ("patient.identifier=synthetic-1", ListRefusal::Unqualified),
        (
            "patient.identifier=%7Csynthetic-1",
            ListRefusal::Unqualified,
        ),
        ("patient.identifier=a%7Cb,a%7Cc", ListRefusal::Several),
        (
            "patient.identifier=a%7Cb&patient.identifier=a%7Cc",
            ListRefusal::Several,
        ),
        (
            "patient.identifier=a%7Cb&status:not=current",
            ListRefusal::Parameter,
        ),
        (
            "patient.identifier=a%7Cb&relatesto=DocumentReference%2F1",
            ListRefusal::Parameter,
        ),
        (
            "patient.identifier=a%7Cb&date=2026-13",
            ListRefusal::Value(CriterionError::Date {
                name: "date",
                source: DateError::Form,
            }),
        ),
        (
            "patient.identifier=a%7Cb&contenttype=a%7Cb%7Cc",
            ListRefusal::Value(CriterionError::Token {
                name: "contenttype",
                source: TokenError::Bars,
            }),
        ),
        (
            "patient.identifier=a%7Cb&type:text=summary",
            ListRefusal::Parameter,
        ),
        ("patient:Patient=1", ListRefusal::Parameter),
        ("patient=Patient%2F1,Patient%2F2", ListRefusal::Several),
        (
            "patient=Patient%2F1&patient.identifier=a%7Cb",
            ListRefusal::Several,
        ),
        ("patient.identifier=a%7Cb&_format=xml", ListRefusal::Format),
    ] {
        assert_eq!(
            ListRequest::from_query(Some(query)),
            Err(refusal),
            "{query}"
        );
    }
}

#[test]
fn the_discharge_codes_every_type_admits_are_read_in_order() {
    for (query, codes) in [
        ("patient.identifier=a%7Cb", &["18842-5", "100719-4"][..]),
        (
            "patient.identifier=a%7Cb&type=http%3A%2F%2Floinc.org%7C",
            &["18842-5", "100719-4"],
        ),
        (
            "patient.identifier=a%7Cb&type=100719-4,http%3A%2F%2Floinc.org%7C18842-5",
            &["18842-5", "100719-4"],
        ),
        ("patient.identifier=a%7Cb&type=100719-4", &["100719-4"]),
        (
            "patient.identifier=a%7Cb&type=18842-5,100719-4&type=18842-5",
            &["18842-5"],
        ),
        ("patient.identifier=a%7Cb&type=60591-5", &[]),
        (
            "patient.identifier=a%7Cb&type=http%3A%2F%2Fexample.org%7C18842-5",
            &[],
        ),
        ("patient.identifier=a%7Cb&type=18842-5&type=100719-4", &[]),
    ] {
        let read = ListRequest::from_query(Some(query)).unwrap();
        assert_eq!(read.discharge(), codes, "{query}");
    }
}

#[test]
fn the_imaging_codes_every_type_admits_are_read_in_order() {
    for (query, codes) in [
        ("patient.identifier=a%7Cb", &["85430-7", "18748-4"][..]),
        (
            "patient.identifier=a%7Cb&type=http%3A%2F%2Floinc.org%7C",
            &["85430-7", "18748-4"],
        ),
        ("patient.identifier=a%7Cb&type=18748-4", &["18748-4"]),
        (
            "patient.identifier=a%7Cb&type=18748-4,http%3A%2F%2Floinc.org%7C85430-7",
            &["85430-7", "18748-4"],
        ),
        ("patient.identifier=a%7Cb&type=18842-5", &[]),
        (
            "patient.identifier=a%7Cb&type=http%3A%2F%2Fexample.org%7C85430-7",
            &[],
        ),
    ] {
        let read = ListRequest::from_query(Some(query)).unwrap();
        assert_eq!(read.imaging(), codes, "{query}");
    }
}

#[test]
fn no_debug_shows_the_identifier() {
    let read = ListRequest::from_query(Some("patient.identifier=a%7Csynthetic-77q")).unwrap();
    assert!(!format!("{read:?}").contains("77q"));
}

/// A member's `DocumentReference`, its members in the order a serialiser
/// writes them, with two attachments and a decimal of two places.
const MEMBERS: &str = r#"{"content":[{"attachment":{"contentType":"application/fhir+json","size":2048,"url":"Binary/lab-1"}},{"attachment":{"title":"no url"},"format":{"code":"urn:x"}}],"extension":[{"url":"urn:oid:2.999.9","valueDecimal":1.50}],"id":"doc-1","resourceType":"DocumentReference","status":"current"}"#;

#[test]
fn each_attachment_url_is_read_in_order() {
    let document: Value = serde_json::from_str(MEMBERS).unwrap();
    assert_eq!(vec!["Binary/lab-1"], attachment_urls(&document));
}

#[test]
fn a_readdressed_document_changes_its_attachment_url_alone() {
    let document: Value = serde_json::from_str(MEMBERS).unwrap();
    let readdressed = readdressed(&document, |url| Some(format!("Binary/token-of-{url}"))).unwrap();
    assert_eq!(
        serde_json::to_string(&readdressed).unwrap(),
        MEMBERS.replace("\"Binary/lab-1\"", "\"Binary/token-of-Binary/lab-1\""),
        "every other element byte for byte, the decimal's text kept"
    );
}

#[test]
fn a_url_with_no_readdress_leaves_no_document() {
    let document: Value = serde_json::from_str(MEMBERS).unwrap();
    assert!(readdressed(&document, |_| None).is_none());
}

#[test]
fn a_passed_document_is_listed_as_the_member_holds_it() {
    let document: Value = serde_json::from_str(MEMBERS).unwrap();
    let bundle = searchset(
        vec![("urn:uuid:1".to_owned(), Match::Passed(document))],
        Vec::new(),
    );
    let text = serde_json::to_string(&bundle).unwrap();
    assert!(text.contains(MEMBERS), "{text}");
    assert!(!format!("{:?}", Match::Passed(Value::Null)).contains("doc-1"));
}
