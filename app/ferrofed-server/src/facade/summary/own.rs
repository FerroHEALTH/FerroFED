// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The documents a document list asks members for at their own endpoints
//! (Regulation (EU) 2025/327 Art 14(1)(d) to (f), Annex II 2.1), and how
//! their answers settle the list's completeness (§11.3, §11.4).
//!
//! A member declared `native` for `Laboratory-Reports`,
//! `Discharge-Reports` or `Medical-Imaging` is asked at its own endpoint, by its own `ehr_id`
//! alone (§5.4.1, N33), with the interactions the HL7 Europe EU Health Data
//! API 1.0.0-ballot `CapabilityStatement-EEHRxF-DocumentAccessProvider`
//! SHALL support: `Patient?identifier=` for its patient's logical id, then
//! `DocumentReference?patient=&type=` for its documents of the category,
//! each searchset followed to its last page.

use std::sync::Arc;

use eehrxf::document::discharge::HDR_BUNDLE;
use eehrxf::document::imaging::IMAGING_BUNDLE;
use eehrxf::document::laboratory::LAB_BUNDLE;
use ehds_logging::category::Category;
use ferrofed_eehrxf::list::{LABORATORY_REPORT, LOINC, RESOURCE, attachment_urls};
use ferrofed_engine::dispatch::DispatchOptions;
use ferrofed_engine::fanout::{Completion, FanOutError};
use ferrofed_engine::hygiene::Withheld;
use ferrofed_engine::single_node::native_search::{By, Next};
use ferrofed_registry::snapshot::RegistrySnapshot;
use http::StatusCode;
use tokio::task::JoinSet;
use url::Url;

use super::laboratory::Laboratory;
use crate::facade::answer::Failure;
use crate::facade::located::Located;
use crate::facade::native::{self, Chosen};
use crate::facade::prescriptions::{Held, Unread, paged};
use crate::facade::request::Arrived;
use crate::federation::Federation;

/// The resource type a member's patient is found by.
const PATIENT: &str = "Patient";

/// A category whose documents a member can serve itself to the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Kind {
    /// Laboratory reports, Art 14(1)(e).
    Laboratory,
    /// Discharge reports, Art 14(1)(f).
    Discharge,
    /// Medical imaging studies and related reports, Art 14(1)(d).
    Imaging,
}

impl Kind {
    /// The priority category.
    pub(crate) const fn category(self) -> Category {
        match self {
            Self::Laboratory => Category::MedicalTestResult,
            Self::Discharge => Category::DischargeReport,
            Self::Imaging => Category::MedicalImaging,
        }
    }

    /// The code of the priority category, such as `Discharge-Reports`.
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::Laboratory => eehrxf::category::Category::Laboratory.code(),
            Self::Discharge => eehrxf::category::Category::Discharge.code(),
            Self::Imaging => eehrxf::category::Category::Imaging.code(),
        }
    }

    /// The document `Bundle` profile the member's documents are asked in.
    pub(crate) const fn profile(self) -> &'static str {
        match self {
            Self::Laboratory => LAB_BUNDLE,
            Self::Discharge => HDR_BUNDLE,
            Self::Imaging => IMAGING_BUNDLE,
        }
    }

    /// The documents, in the words an outcome names them by.
    pub(crate) const fn documents(self) -> &'static str {
        match self {
            Self::Laboratory => "laboratory reports",
            Self::Discharge => "discharge reports",
            Self::Imaging => "imaging reports",
        }
    }

    /// The kind of the category of the code `code`, such as
    /// `Discharge-Reports`.
    pub(crate) fn of_category(code: &str) -> Option<Self> {
        [Self::Laboratory, Self::Discharge, Self::Imaging]
            .into_iter()
            .find(|kind| kind.code() == code)
    }
}

/// The documents a list names beside the summaries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Wanted {
    /// The laboratory reports.
    pub(crate) laboratory: Laboratory,
    /// The LOINC codes of the discharge reports the members that serve them
    /// themselves are asked for; none asks them nothing.
    pub(crate) discharge: Vec<&'static str>,
    /// The LOINC codes of the imaging documents the members that serve them
    /// themselves are asked for; none asks them nothing.
    pub(crate) imaging: Vec<&'static str>,
}

impl Wanted {
    /// Whether it names any document.
    pub(crate) fn any(&self) -> bool {
        self.laboratory != Laboratory::Unlisted
            || !self.discharge.is_empty()
            || !self.imaging.is_empty()
    }
}

/// A member asked for its own documents of a kind, by the LOINC codes of
/// their types.
#[derive(Debug)]
pub(crate) struct Asking {
    /// The kind.
    pub(crate) kind: Kind,
    /// The codes, each of the LOINC system.
    pub(crate) codes: Vec<&'static str>,
    /// The member.
    pub(crate) chosen: Chosen,
}

/// Every member asked for its own documents: the members of `laboratory`
/// for their laboratory reports, and, for each kind `wanted` names types
/// of, every member `located` dispatches to that serves that kind itself at
/// an endpoint `snapshot` holds, for its documents of those types.
pub(super) fn asking(
    snapshot: &RegistrySnapshot,
    located: &Located,
    laboratory: Vec<Chosen>,
    wanted: &Wanted,
) -> Vec<Asking> {
    let mut asking: Vec<Asking> = laboratory
        .into_iter()
        .map(|chosen| Asking {
            kind: Kind::Laboratory,
            codes: vec![LABORATORY_REPORT],
            chosen,
        })
        .collect();
    for (kind, codes) in [
        (Kind::Discharge, &wanted.discharge),
        (Kind::Imaging, &wanted.imaging),
    ] {
        if codes.is_empty() {
            continue;
        }
        let chosen = native::chosen_for(snapshot, &located.targets, kind.code());
        asking.extend(chosen.into_iter().map(|chosen| Asking {
            kind,
            codes: codes.clone(),
            chosen,
        }));
    }
    asking
}

/// A member that serves documents of a kind itself, and what its own
/// endpoint answered.
#[derive(Debug)]
pub(crate) struct Own {
    /// The kind.
    pub(crate) kind: Kind,
    /// The member, as the list reached it.
    pub(crate) chosen: Chosen,
    /// Every page of its `DocumentReference` search, or why there is none.
    pub(crate) held: Held,
}

/// Asks every member of `asking` for its documents at its own endpoint, at
/// once, under the budget of the request `arrived`, refusing to send
/// anything carrying one of `withheld`, in the order of `asking`.
///
/// # Errors
///
/// [`Failure::FanOut`] when the clock cannot hold the budget, when a member
/// has no client or no native endpoint, or when a task failed.
pub(super) async fn asked(
    federation: &Arc<Federation>,
    arrived: &Arrived<'_>,
    asking: Vec<Asking>,
    withheld: Arc<Withheld>,
) -> Result<Vec<Own>, Failure> {
    let (options, until) =
        native::options(federation, arrived, withheld).map_err(Failure::FanOut)?;
    let mut tasks = JoinSet::new();
    for (position, member) in asking.into_iter().enumerate() {
        let federation = Arc::clone(federation);
        let options = options.clone();
        tasks.spawn(async move {
            let held = documents(&federation, &member, &options, until).await;
            held.map(|held| {
                (
                    position,
                    Own {
                        kind: member.kind,
                        chosen: member.chosen,
                        held,
                    },
                )
            })
        });
    }
    let mut answered = Vec::new();
    while let Some(joined) = tasks.join_next().await {
        let own = joined
            .map_err(|joined| Failure::FanOut(FanOutError::Task(joined)))?
            .map_err(Failure::FanOut)?;
        if let Held::Unusable { status, reason } = &own.1.held {
            tracing::warn!(
                endpoint = own.1.chosen.native.as_str(),
                category = own.1.kind.code(),
                status = status.as_u16(),
                reason = %reason,
                request_id = %arrived.outbound,
                "a member answered documents the list cannot pass through"
            );
        }
        answered.push(own);
    }
    answered.sort_by_key(|(position, _)| *position);
    Ok(answered.into_iter().map(|(_, own)| own).collect())
}

/// What `asking`'s member answered for its documents, under `options`,
/// abandoned when the overall budget `until` runs out: no page when it
/// holds no `Patient` for its `ehr_id`.
async fn documents(
    federation: &Federation,
    asking: &Asking,
    options: &DispatchOptions,
    until: tokio::time::Instant,
) -> Result<Held, FanOutError> {
    let member = &asking.chosen;
    let patients = paged(
        federation,
        (member, PATIENT, By::Identifier),
        options,
        until,
    )
    .await?;
    let Held::Found(pages) = patients else {
        return Ok(patients);
    };
    let ids: Vec<Option<&str>> = pages
        .iter()
        .flat_map(|page| &page.prescriptions)
        .map(|found| found.kept.id())
        .collect();
    let patient = match ids.as_slice() {
        // NOTE: FHIR R4 search: an empty searchset says the member holds no Patient for its ehr_id,
        // so it holds no document of the patient either.
        [] => return Ok(Held::Found(Vec::new())),
        [Some(id)] if logical(id) => (*id).to_owned(),
        _ => {
            return Ok(Held::Unusable {
                status: StatusCode::OK,
                reason: Unread::Patient,
            });
        }
    };
    // NOTE: FHIR R4 search, AND and OR: a comma separates the values one parameter ORs, which
    // every search parameter takes, so one search asks every type of the category.
    let kind = asking
        .codes
        .iter()
        .map(|code| format!("{LOINC}|{code}"))
        .collect::<Vec<_>>()
        .join(",");
    let by = By::Patient {
        patient: &patient,
        kind: &kind,
    };
    let documents = paged(federation, (member, RESOURCE, by), options, until).await?;
    let Held::Found(pages) = &documents else {
        return Ok(documents);
    };
    let Some(base) = federation
        .snapshot()
        .native_endpoint(&member.native)
        .map(|native| native.url().clone())
    else {
        return Err(FanOutError::UnknownEndpoint {
            endpoint: member.via.clone(),
        });
    };
    let readable = pages
        .iter()
        .flat_map(|page| &page.prescriptions)
        .flat_map(|document| attachment_urls(&document.kept.resource))
        .all(|url| binary(&base, url).is_some());
    if !readable {
        return Ok(Held::Unusable {
            status: StatusCode::OK,
            reason: Unread::Attachment,
        });
    }
    Ok(documents)
}

/// The logical id of the member's `Binary` that `url`, an attachment URL of
/// one of its `DocumentReference`s, names under its base `base`: the read
/// `Binary/{id}`, relative to the base or absolute under it, with no query
/// or fragment, or `None` for any other URL.
pub(crate) fn binary(base: &Url, url: &str) -> Option<String> {
    let mut root = base.clone();
    if !root.path().ends_with('/') {
        root.set_path(&format!("{}/", root.path()));
    }
    let named = root.join(url).ok()?;
    if named.query().is_some() {
        return None;
    }
    let under = Next::under(base, &named).ok()?;
    let id = under.path().strip_prefix("/Binary/")?;
    logical(id).then(|| id.to_owned())
}

/// Whether `id` is a FHIR R4 `id`, `[A-Za-z0-9\-\.]{1,64}`, other than a dot
/// segment, so it names one resource in a path.
fn logical(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'.')
        && id != "."
        && id != ".."
}

/// The status `status` becomes once the members' own answers `own` are
/// settled under `completion`: a silent member fails it under
/// all-or-nothing, `504` for one in `time-out` or `offline` and `424`
/// otherwise (§11.3, §11.4).
pub(super) fn settled(own: &[Own], status: StatusCode, completion: Completion) -> StatusCode {
    super::laboratory::settled_by(own.iter().map(|own| own.held.status()), status, completion)
}

#[cfg(test)]
mod tests {
    use super::{Kind, binary};
    use url::Url;

    #[test]
    fn a_binary_under_the_members_base_is_read_by_its_id() {
        let base = Url::parse("https://member.example.org/fhir").unwrap();
        for url in [
            "Binary/lab-1",
            "https://member.example.org/fhir/Binary/lab-1",
            "./Binary/lab-1",
        ] {
            assert_eq!(Some("lab-1".to_owned()), binary(&base, url), "{url}");
        }
    }

    #[test]
    fn every_other_attachment_url_names_no_binary() {
        let base = Url::parse("https://member.example.org/fhir/").unwrap();
        for url in [
            "https://other.example.org/fhir/Binary/lab-1",
            "https://member.example.org/Binary/lab-1",
            "Binary/lab-1?_format=json",
            "Binary/lab-1#part",
            "Binary/",
            "Binary/a/b",
            "Binary/..",
            "DocumentReference/lab-1",
            "Binary/lab%2F1",
            "urn:uuid:5a1e0000-0000-4000-8000-00000000000a",
        ] {
            assert_eq!(None, binary(&base, url), "{url}");
        }
    }

    #[test]
    fn each_kind_is_found_by_its_category_code() {
        for kind in [Kind::Laboratory, Kind::Discharge, Kind::Imaging] {
            assert_eq!(Some(kind), Kind::of_category(kind.code()));
            assert_eq!(kind.code(), kind.category().code());
        }
        assert_eq!(None, Kind::of_category("Patient-Summaries"));
    }
}
