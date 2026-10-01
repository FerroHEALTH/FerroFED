// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The synthetic seed builder: EHRs, the template and compositions, written
//! to a node through its own ITS-REST API and nothing else.
//!
//! Every value is invented for the test. A patient identifier is a
//! [`PatientId`], which can only be built inside the example arc
//! `urn:oid:2.999.1.<n>` that ITU-T X.660 and ISO/IEC 9834 reserve for
//! examples, with a non-numeric value (`ffd-test-0001`) that no national
//! identifier scheme validates. A seed never reaches into a node's database,
//! so the harness is the same for every CDR product (`docs/architecture.md`
//! section 13): `PUT {api}/v1/ehr/{ehr_id}`,
//! `POST {api}/v1/definition/template/adl1.4` and
//! `POST {api}/v1/ehr/{ehr_id}/composition`, as openEHR ITS-REST 1.1.0 names
//! them.
//!
//! [`feed`] plays the Patient Identity Source of ITI-104 against the harness
//! PIX Manager ([`crate::pix`]): one `Patient` per synthetic patient, carrying
//! its identifier and the `ehr_id` it has at each node, each node's `ehr_id`s
//! being one identity domain, an [`EhrDomain`], inside the example arc
//! `urn:oid:2.999.2.<n>`.
//!
//! The template and the compositions are the reference implementation's
//! demo data, vendored under `docs/specs/federation-ref/docker/demo-data/`:
//! the `International Patient Summary` operational template and four
//! canonical-JSON compositions whose only party is `PARTY_SELF`.

use http::StatusCode;
use http::header::{ACCEPT, CONTENT_TYPE, ETAG};
use serde::Serialize;
use std::path::PathBuf;
use uuid::Uuid;

/// The example arc every synthetic patient identifier's namespace lives in.
pub const EXAMPLE_ARC: &str = "urn:oid:2.999.1";

/// The example arc every node's `ehr_id` domain at the PIX Manager lives in.
pub const EHR_DOMAIN_ARC: &str = "urn:oid:2.999.2";

/// The vendored demo data the template and the compositions come from.
const DEMO_DATA: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/federation-ref/docker/demo-data"
);

/// The template id of the vendored operational template.
pub const TEMPLATE_ID: &str = "International Patient Summary";

/// A synthetic patient identifier: an assigning domain inside the example
/// arc and a non-numeric value.
///
/// There is no way to build one outside the arc, so a fixture cannot carry a
/// real national identifier by accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PatientId {
    /// The assigning domain, the last arc of the namespace.
    domain: u16,
    /// The patient's number inside that domain.
    number: u16,
}

impl PatientId {
    /// Returns the identifier of patient `number` in assigning domain
    /// `domain`.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrofed_testkit::seed::PatientId;
    ///
    /// let patient = PatientId::new(1, 7);
    /// assert_eq!(patient.namespace(), "urn:oid:2.999.1.1");
    /// assert_eq!(patient.value(), "ffd-test-0007");
    /// ```
    #[must_use]
    pub const fn new(domain: u16, number: u16) -> Self {
        Self { domain, number }
    }

    /// Returns the namespace, the assigning domain's OID in the example arc.
    #[must_use]
    pub fn namespace(self) -> String {
        format!("{EXAMPLE_ARC}.{}", self.domain)
    }

    /// Returns the identifier value inside that namespace.
    #[must_use]
    pub fn value(self) -> String {
        format!("ffd-test-{:04}", self.number)
    }
}

/// A node's `ehr_id` domain at the PIX Manager.
///
/// It is the assigning authority whose identifier values are that node's
/// `ehr_id`s (Federation Tier with AQL Annex A.1, `targetSystem=<the domain's
/// ehr_id system>`). There is no way to build one outside the example arc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EhrDomain(u16);

impl EhrDomain {
    /// Returns the `ehr_id` domain of harness node `node`.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrofed_testkit::seed::EhrDomain;
    ///
    /// assert_eq!(EhrDomain::new(1).system(), "urn:oid:2.999.2.1");
    /// ```
    #[must_use]
    pub const fn new(node: u16) -> Self {
        Self(node)
    }

    /// Returns the domain's system, the value a gateway's `[pixm]` member
    /// mapping names and an ITI-83 `targetSystem` carries.
    #[must_use]
    pub fn system(self) -> String {
        format!("{EHR_DOMAIN_ARC}.{}", self.0)
    }
}

/// One synthetic patient's cross-reference, as the ITI-104 feed delivers it to
/// the PIX Manager: the patient's identifier and the `ehr_id` it has at each
/// node that holds its record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossReferenceSeed {
    /// The patient.
    pub patient: PatientId,
    /// The patient's `ehr_id` in each node's domain.
    pub ehrs: Vec<(EhrDomain, Uuid)>,
}

/// One of the four vendored demo compositions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DemoComposition {
    /// The first patient's summary as a clinic records it.
    FirstClinic,
    /// The first patient's summary as a hospital records it.
    FirstHospital,
    /// The second patient's summary as a clinic records it.
    SecondClinic,
    /// The second patient's summary as a hospital records it.
    SecondHospital,
}

impl DemoComposition {
    /// Every demo composition.
    pub const ALL: [Self; 4] = [
        Self::FirstClinic,
        Self::FirstHospital,
        Self::SecondClinic,
        Self::SecondHospital,
    ];

    /// Returns the vendored file the composition is read from.
    #[must_use]
    pub fn path(self) -> PathBuf {
        let file = match self {
            Self::FirstClinic => "composition-12345-clinic.json",
            Self::FirstHospital => "composition-12345-hospital.json",
            Self::SecondClinic => "composition-67890-clinic.json",
            Self::SecondHospital => "composition-67890-hospital.json",
        };
        PathBuf::from(DEMO_DATA).join(file)
    }
}

/// Returns the path of the vendored operational template.
#[must_use]
pub fn template_path() -> PathBuf {
    PathBuf::from(DEMO_DATA).join(format!("{TEMPLATE_ID}.opt"))
}

/// An EHR to create on a node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EhrSeed {
    /// The `ehr_id` the node is asked to create, fixed per scenario.
    pub ehr_id: Uuid,
    /// The patient the EHR's `EHR_STATUS.subject` names, if any.
    pub subject: Option<PatientId>,
}

/// A composition to commit to an EHR on a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompositionSeed {
    /// The EHR the composition is committed to.
    pub ehr_id: Uuid,
    /// The vendored composition.
    pub composition: DemoComposition,
}

/// What to write to one node, in order: the EHRs, the template, then the
/// compositions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SeedPlan {
    /// The EHRs to create.
    pub ehrs: Vec<EhrSeed>,
    /// Whether to upload the vendored operational template.
    pub template: bool,
    /// The compositions to commit.
    pub compositions: Vec<CompositionSeed>,
}

/// A composition the node accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Committed {
    /// The EHR it was committed to.
    pub ehr_id: Uuid,
    /// The version uid the node named in its `ETag`, without the quotes.
    pub version_uid: Option<String>,
}

/// What a seed wrote.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SeedReport {
    /// The EHRs created, in plan order.
    pub ehrs: Vec<Uuid>,
    /// The compositions committed, in plan order.
    pub compositions: Vec<Committed>,
}

/// A seed step failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SeedError {
    /// A vendored fixture could not be read.
    #[error("the vendored fixture {} could not be read", path.display())]
    Fixture {
        /// The fixture that was read.
        path: PathBuf,
        /// What the file system reported.
        #[source]
        source: std::io::Error,
    },
    /// The `EHR_STATUS` body could not be serialised.
    #[error("the EHR_STATUS body could not be serialised")]
    Body(#[source] serde_json::Error),
    /// The ITS-REST client could not be built.
    #[error("the ITS-REST client could not be built")]
    Client(#[source] reqwest::Error),
    /// A request could not be sent or its answer not read.
    #[error("{step} could not be sent")]
    Send {
        /// The ITS-REST call, method and path.
        step: String,
        /// What the HTTP stack reported.
        #[source]
        source: reqwest::Error,
    },
    /// The PIX Manager's base URL is unusable.
    #[error(transparent)]
    PixBase(#[from] PixBaseError),
    /// The node refused a step.
    #[error("{step} was answered {status}: {detail}")]
    Refused {
        /// The ITS-REST call, method and path.
        step: String,
        /// The status the node answered.
        status: StatusCode,
        /// The node's error body, as text, for the test log. Seeds carry
        /// synthetic data only, so the body holds nothing that may not be
        /// shown.
        detail: String,
    },
}

/// The PIX Manager's base URL is not a URL a FHIR path can be joined to.
#[derive(Debug, thiserror::Error)]
#[error("the PIX Manager base URL could not be joined with Patient")]
pub struct PixBaseError(#[source] url::ParseError);

/// Writes `plan` to the node whose ITS-REST API root is `api_root` (the URL
/// `/v1/ehr` lives under, for example `http://host:port/ehrbase/rest/openehr`).
///
/// Every step must succeed: a node that already holds an EHR or the template
/// answers `409`, which is a refusal here, because a seed runs against a
/// freshly started node and a collision means the scenario is wrong.
///
/// # Errors
///
/// Returns [`SeedError::Fixture`] when a vendored file cannot be read,
/// [`SeedError::Send`] when a request cannot be sent, and
/// [`SeedError::Refused`] when the node answers anything but success.
pub async fn seed(api_root: &str, plan: &SeedPlan) -> Result<SeedReport, SeedError> {
    let api_root = api_root.trim_end_matches('/');
    let client = reqwest::Client::builder()
        .build()
        .map_err(SeedError::Client)?;
    let mut report = SeedReport::default();

    for ehr in &plan.ehrs {
        let body =
            serde_json::to_vec(&EhrStatus::for_subject(ehr.subject)).map_err(SeedError::Body)?;
        let step = format!("PUT /v1/ehr/{}", ehr.ehr_id);
        let request = client
            .put(format!("{api_root}/v1/ehr/{}", ehr.ehr_id))
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json")
            .header("Prefer", "return=minimal")
            .body(body);
        send(request, step).await?;
        report.ehrs.push(ehr.ehr_id);
    }

    if plan.template {
        let path = template_path();
        let body = read(&path)?;
        // EHRbase answers 406 to an upload whose client does not accept XML
        // back, though the answer has no body; FerroEHR accepts either. One
        // request that accepts XML serves both without a per-product case.
        let request = client
            .post(format!("{api_root}/v1/definition/template/adl1.4"))
            .header(CONTENT_TYPE, "application/xml")
            .header(ACCEPT, "application/xml")
            .header("Prefer", "return=minimal")
            .body(body);
        send(request, "POST /v1/definition/template/adl1.4".to_owned()).await?;
    }

    for seed in &plan.compositions {
        let body = read(&seed.composition.path())?;
        let step = format!("POST /v1/ehr/{}/composition", seed.ehr_id);
        let request = client
            .post(format!("{api_root}/v1/ehr/{}/composition", seed.ehr_id))
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json")
            .header("Prefer", "return=minimal")
            .body(body);
        let answer = send(request, step).await?;
        let version_uid = answer
            .headers()
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.trim_matches('"').to_owned());
        report.compositions.push(Committed {
            ehr_id: seed.ehr_id,
            version_uid,
        });
    }

    Ok(report)
}

/// Feeds `cross_reference` to the PIX Manager over ITI-104.
///
/// The seed plays the Patient Identity Source of ITI-104 (Patient Identity
/// Feed FHIR, IHE PIXm 3.1.0) against the Manager whose FHIR base URL, with
/// its trailing slash, is `pix_base`: a conditional update
/// `PUT [base]/Patient?identifier=<namespace>|<value>` of one `Patient`
/// carrying the patient's identifier and its `ehr_id` in each node's domain,
/// with a synthetic name, so the `Patient` holds to the `IHE.PIXm.Patient`
/// profile. Returns the status the Manager answered: `201` when it created the
/// `Patient`, `200` when it replaced one.
///
/// # Errors
///
/// Returns [`SeedError::PixBase`] when `pix_base` cannot be joined with
/// `Patient`, [`SeedError::Body`] when the `Patient` cannot be serialised,
/// [`SeedError::Send`] when the request cannot be sent, and
/// [`SeedError::Refused`] when the Manager answers anything but success.
pub async fn feed(
    pix_base: &str,
    cross_reference: &CrossReferenceSeed,
) -> Result<StatusCode, SeedError> {
    use fhir_types::r4::human_name::HumanName;
    use fhir_types::r4::identifier::Identifier;
    use fhir_types::r4::patient::Patient;

    let patient = cross_reference.patient;
    let identifier = |system: &str, value: &str| Identifier {
        system: Some(system.into()),
        value: Some(value.into()),
        ..Identifier::default()
    };
    let mut identifiers = vec![identifier(&patient.namespace(), &patient.value())];
    for (domain, ehr_id) in &cross_reference.ehrs {
        identifiers.push(identifier(&domain.system(), &ehr_id.to_string()));
    }
    let body = Patient {
        identifier: identifiers,
        name: vec![HumanName {
            family: Some("Synthetic".into()),
            given: vec![patient.value().as_str().into()],
            ..HumanName::default()
        }],
        ..Patient::default()
    };
    let body = serde_json::to_vec(&body).map_err(SeedError::Body)?;
    let mut url = url::Url::parse(pix_base)
        .and_then(|base| base.join("Patient"))
        .map_err(PixBaseError)?;
    url.query_pairs_mut().append_pair(
        "identifier",
        &format!("{}|{}", patient.namespace(), patient.value()),
    );
    let client = reqwest::Client::builder()
        .build()
        .map_err(SeedError::Client)?;
    let request = client
        .put(url)
        .header(CONTENT_TYPE, "application/fhir+json")
        .header(ACCEPT, "application/fhir+json")
        .body(body);
    let answer = send(request, "PUT /Patient?identifier= (ITI-104)".to_owned()).await?;
    Ok(answer.status())
}

/// Sends `request` and returns its answer when the node accepted it.
async fn send(
    request: reqwest::RequestBuilder,
    step: String,
) -> Result<reqwest::Response, SeedError> {
    let answer = request.send().await.map_err(|source| SeedError::Send {
        step: step.clone(),
        source,
    })?;
    let status = answer.status();
    if status.is_success() {
        return Ok(answer);
    }
    let detail = answer.text().await.unwrap_or_default();
    Err(SeedError::Refused {
        step,
        status,
        detail,
    })
}

/// Reads a vendored fixture.
fn read(path: &PathBuf) -> Result<Vec<u8>, SeedError> {
    std::fs::read(path).map_err(|source| SeedError::Fixture {
        path: path.clone(),
        source,
    })
}

/// The canonical-JSON `EHR_STATUS` a `PUT /ehr/{ehr_id}` carries.
#[derive(Debug, Serialize)]
pub struct EhrStatus {
    #[serde(rename = "_type")]
    kind: &'static str,
    archetype_node_id: &'static str,
    name: DvText,
    archetype_details: Archetyped,
    subject: PartySelf,
    is_queryable: bool,
    is_modifiable: bool,
}

impl EhrStatus {
    /// Returns the status of an EHR whose subject is `subject`, or an
    /// anonymous `PARTY_SELF` when there is none.
    #[must_use]
    pub fn for_subject(subject: Option<PatientId>) -> Self {
        Self {
            kind: "EHR_STATUS",
            archetype_node_id: "openEHR-EHR-EHR_STATUS.generic.v1",
            name: DvText {
                kind: "DV_TEXT",
                value: "EHR Status",
            },
            // NOTE: RM 1.1.0 LOCATABLE invariant Archetyped_valid; an
            // EHR_STATUS is an archetype root, so it names its archetype.
            archetype_details: Archetyped {
                kind: "ARCHETYPED",
                archetype_id: ArchetypeId {
                    kind: "ARCHETYPE_ID",
                    value: "openEHR-EHR-EHR_STATUS.generic.v1",
                },
                rm_version: "1.1.0",
            },
            subject: PartySelf {
                kind: "PARTY_SELF",
                external_ref: subject.map(|patient| PartyRef {
                    kind: "PARTY_REF",
                    id: GenericId {
                        kind: "GENERIC_ID",
                        value: patient.value(),
                        scheme: "ffd-test",
                    },
                    namespace: patient.namespace(),
                    party_type: "PERSON",
                }),
            },
            is_queryable: true,
            is_modifiable: true,
        }
    }
}

#[derive(Debug, Serialize)]
struct DvText {
    #[serde(rename = "_type")]
    kind: &'static str,
    value: &'static str,
}

#[derive(Debug, Serialize)]
struct Archetyped {
    #[serde(rename = "_type")]
    kind: &'static str,
    archetype_id: ArchetypeId,
    rm_version: &'static str,
}

#[derive(Debug, Serialize)]
struct ArchetypeId {
    #[serde(rename = "_type")]
    kind: &'static str,
    value: &'static str,
}

#[derive(Debug, Serialize)]
struct PartySelf {
    #[serde(rename = "_type")]
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    external_ref: Option<PartyRef>,
}

#[derive(Debug, Serialize)]
struct PartyRef {
    #[serde(rename = "_type")]
    kind: &'static str,
    id: GenericId,
    namespace: String,
    #[serde(rename = "type")]
    party_type: &'static str,
}

#[derive(Debug, Serialize)]
struct GenericId {
    #[serde(rename = "_type")]
    kind: &'static str,
    value: String,
    scheme: &'static str,
}
