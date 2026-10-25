// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! How a [`Document`] writes its form's type, categories, authors and report
//! (no specification governs the split: our own design, each rule citing the
//! profile it follows).

use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::composition::Composition;
use fhir_types::r4::diagnostic_report::DiagnosticReport;
use fhir_types::r4::extension::{Extension, ExtensionValue};
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::meta::Meta;
use fhir_types::r4::reference::Reference;

use crate::document::{
    Authorship, Companion, Document, DocumentError, Kind, URN_SYSTEM, coding, reference,
};

impl Document {
    /// Writes the form's type, categories and authors on `composition`, with
    /// `report`, the report the form holds, when it has one.
    ///
    /// The report's categories are the composition's too
    /// (`Bundle-eu-lab` `dr-comp-category`). Under
    /// [`Authorship::ReportPerformers`] the composition's `Organization`
    /// authors are the report's performing organizations
    /// (`BundleReportEuImaging` `dr-comp-author-org`).
    pub(super) fn arrange(&self, composition: &mut Composition, report: Option<&DiagnosticReport>) {
        composition.r#type.coding = match (self.form.kind, report) {
            (Kind::Fixed(kind), _) => vec![coding(kind)],
            (Kind::Report { .. }, Some(report)) if !report.code.coding.is_empty() => {
                report.code.coding.clone()
            }
            (Kind::Report { fallback }, _) => vec![coding(fallback)],
        };
        composition.category = match report {
            Some(report) => report.category.clone(),
            None => self
                .form
                .categories
                .iter()
                .map(|category| CodeableConcept {
                    coding: vec![coding(*category)],
                    ..CodeableConcept::default()
                })
                .collect(),
        };
        if self.form.authorship == Authorship::ReportPerformers {
            let (organizations, others): (Vec<Reference>, Vec<Reference>) =
                composition.author.drain(..).partition(names_organization);
            composition.custodian = organizations.into_iter().next();
            composition.author = others;
            composition.author.extend(
                report
                    .into_iter()
                    .flat_map(|report| report.performer.iter())
                    .filter(|performer| names_organization(performer))
                    .cloned(),
            );
        }
    }

    /// The report entry of a form that holds one, about `subject` and
    /// naming the composition at `composition_url`.
    pub(super) fn report_written(
        &mut self,
        subject: &str,
        composition_url: &str,
    ) -> Result<Option<(String, DiagnosticReport)>, DocumentError> {
        match (self.form.report, self.report.take()) {
            (Some(companion), Some(report)) => Ok(Some(self.report_entry(
                companion,
                report,
                subject,
                composition_url,
            ))),
            (None, None) => Ok(None),
            (Some(_), None) => Err(DocumentError::Report {
                expected: "requires",
            }),
            (None, Some(_)) => Err(DocumentError::Report {
                expected: "holds no",
            }),
        }
    }

    /// The `fullUrl` and the content of the report entry: `report` with the
    /// profile of `companion`, the document's identifier, its type among the
    /// codings, `subject`, and the extension naming the composition at
    /// `composition_url`.
    fn report_entry(
        &self,
        companion: Companion,
        mut report: DiagnosticReport,
        subject: &str,
        composition_url: &str,
    ) -> (String, DiagnosticReport) {
        let id = report.id.clone().unwrap_or_else(|| String::from("report"));
        report.id = Some(id.clone());
        report
            .meta
            .get_or_insert_with(Meta::default)
            .profile
            .push(companion.profile.into());
        let identifier = Identifier {
            system: Some(URN_SYSTEM.into()),
            value: Some(self.identifier.as_str().into()),
            ..Identifier::default()
        };
        if !report.identifier.contains(&identifier) {
            report.identifier.push(identifier);
        }
        match self.form.kind {
            Kind::Fixed(kind) => {
                let kind = coding(kind);
                if !report.code.coding.contains(&kind) {
                    report.code.coding.push(kind);
                }
            }
            Kind::Report { fallback } => {
                if report.code.coding.is_empty() {
                    report.code.coding.push(coding(fallback));
                }
            }
        }
        for category in self.form.categories {
            let category = coding(*category);
            if !report
                .category
                .iter()
                .any(|held| held.coding.contains(&category))
            {
                report.category.push(CodeableConcept {
                    coding: vec![category],
                    ..CodeableConcept::default()
                });
            }
        }
        report.subject = Some(reference(subject));
        report.encounter = self.encounter.as_deref().map(reference);
        report
            .extension
            .retain(|extension| extension.url != companion.back);
        report.extension.push(Extension {
            url: companion.back.into(),
            value: Some(ExtensionValue::Reference(Box::new(reference(
                composition_url,
            )))),
            ..Extension::default()
        });
        (self.full_url("DiagnosticReport", &id), report)
    }
}

/// Whether `reference` names an `Organization`, by its literal `Type/id` or
/// `{base}/Type/id` form (FHIR R4 `references.html`).
fn names_organization(reference: &Reference) -> bool {
    reference
        .reference
        .as_ref()
        .and_then(|text| text.value.as_deref())
        .is_some_and(|text| text.starts_with("Organization/") || text.contains("/Organization/"))
}
