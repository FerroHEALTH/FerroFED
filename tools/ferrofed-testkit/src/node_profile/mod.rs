// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Federation-Node profile checks: what a member CDR's ITS-REST interface
//! shows of the node obligations of §16.2.
//!
//! A node is invocable on its local `ehr_id` alone, never requires `subject`,
//! passes its own errors through, enforces consent before it releases data,
//! and meets the identifier-integrity conditions of §12b.2 (§16.2, N34). §17
//! scores CP-18, CP-19 and CP-27 against the node, never the gateway, and a
//! gateway cannot prove them. Each check here exercises what a request to the
//! node's interface can observe and returns a [`Finding`] with its
//! [`Verdict`] and the evidence behind it, which an operator admitting a node
//! reads, and which the harness runs against its CDR products:
//!
//! | Check | Observed at the interface | Point |
//! |---|---|---|
//! | [`checks::invocable_on_ehr_id`] | the EHR, its `EHR_STATUS` and its compositions answer to the `ehr_id` alone | CP-27 |
//! | [`checks::subject_not_required`] | an EHR created with no subject is read and queried by its `ehr_id` | CP-27 |
//! | [`checks::errors_passed_through`] | an unknown `ehr_id` and an unparsable query answer the ITS-REST error status | CP-18 |
//! | [`checks::access_decided_at_node`] | an EHR the node's own policy withholds from a principal is refused on the read and on the `ehr_id`-scoped query | CP-18 |
//! | [`checks::consent_before_release`] | the same, for a refusal the operator arranged as a consent refusal | CP-19 |
//!
//! The §12b.2 conditions are the gateway's admission check, whose findings
//! the harness records under [`Check::IdentifierIntegrity`]. A check that
//! could not arrange what it needs says so with [`Verdict::NotObservable`],
//! never with a pass, and a node it could not reach is a [`CheckError`]: a
//! check that reached nothing concludes nothing.
//!
//! A [`Profile`] collects the findings for one product and writes them as
//! tab-separated rows, which `scripts/conformance/report.sh` reports as the
//! node class, apart from the gateway's points. No specification governs the
//! form of the checks or of their record: our own design.

pub mod checks;

use std::fmt;
use std::path::{Path, PathBuf};

use reqwest::header::{ACCEPT, CONTENT_TYPE, LOCATION};
use uuid::Uuid;

/// The environment variable naming the directory a [`Profile`] is written
/// to, in place of [`DEFAULT_FINDINGS_DIR`].
pub const FINDINGS_DIR_VARIABLE: &str = "FERROFED_NODE_PROFILE_DIR";

/// The directory a [`Profile`] is written to by default, relative to the
/// workspace root, beside the conformance report.
pub const DEFAULT_FINDINGS_DIR: &str = "target/conformance/node-profile";

/// The header line of a findings file.
pub const FINDINGS_HEADER: &str = "product\tpoint\tcheck\tverdict\tevidence";

/// One obligation of the Federation-Node profile, as a check observes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// The node is invocable on its local `ehr_id` alone (§5.5, N34).
    InvocableOnEhrId,
    /// The node never requires `subject` (§16.2).
    SubjectNotRequired,
    /// The node passes its own errors through (§16.2).
    ErrorsPassedThrough,
    /// The node makes the access decisions that need information the gateway
    /// cannot see (§13, N26).
    AccessDecidedAtNode,
    /// The node checks consent before it releases data, whatever was decided
    /// upstream (§13.2, N27).
    ConsentBeforeRelease,
    /// One identifier-integrity condition of §12b.2, as the admission check
    /// reports it.
    IdentifierIntegrity {
        /// The condition, as the §12b.2 table names it.
        condition: &'static str,
        /// The conformance points the condition assists.
        points: &'static [&'static str],
    },
}

impl Check {
    /// Returns the check's name, the column a findings row carries.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::InvocableOnEhrId => "invocable on ehr_id alone",
            Self::SubjectNotRequired => "subject never required",
            Self::ErrorsPassedThrough => "node errors passed through",
            Self::AccessDecidedAtNode => "access decided at the node",
            Self::ConsentBeforeRelease => "consent before release",
            Self::IdentifierIntegrity { condition, .. } => condition,
        }
    }

    /// Returns the §17 conformance points the check assists.
    #[must_use]
    pub fn points(self) -> &'static [&'static str] {
        match self {
            Self::InvocableOnEhrId | Self::SubjectNotRequired => &["CP-27"],
            // NOTE: §16.2 lists error pass-through in the node profile and §17 scores no point for
            // it alone; it assists CP-18, whose delegated decisions reach the gateway as node errors.
            Self::ErrorsPassedThrough | Self::AccessDecidedAtNode => &["CP-18"],
            Self::ConsentBeforeRelease => &["CP-19"],
            Self::IdentifierIntegrity { points, .. } => points,
        }
    }
}

/// What a check concluded about one obligation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// What the interface showed meets the obligation.
    Pass,
    /// What the interface showed breaks it.
    Fail,
    /// The interface showed nothing that decides it; the evidence says why.
    NotObservable,
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::NotObservable => "not-observable",
        })
    }
}

/// The verdict on one obligation and the evidence for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The obligation.
    check: Check,
    /// The verdict.
    verdict: Verdict,
    /// One line per observation, in the order the check made them.
    evidence: Vec<String>,
}

impl Finding {
    /// Creates the finding on `check` with `verdict` and its `evidence`.
    #[must_use]
    pub fn new(check: Check, verdict: Verdict, evidence: Vec<String>) -> Self {
        Self {
            check,
            verdict,
            evidence,
        }
    }

    /// Creates the finding on `check` from its observations: one that fails
    /// fails it, else one that decides nothing leaves it not observable, else
    /// it passes. No observation at all leaves it not observable.
    #[must_use]
    pub fn from_observations(check: Check, observations: Vec<(Verdict, String)>) -> Self {
        let verdict = if observations.iter().any(|(v, _)| *v == Verdict::Fail) {
            Verdict::Fail
        } else if observations.is_empty()
            || observations
                .iter()
                .any(|(v, _)| *v == Verdict::NotObservable)
        {
            Verdict::NotObservable
        } else {
            Verdict::Pass
        };
        Self::new(
            check,
            verdict,
            observations.into_iter().map(|(_, line)| line).collect(),
        )
    }

    /// Creates the finding on `check` that nothing was arranged to observe
    /// it, for the `reason` given.
    #[must_use]
    pub fn not_arranged(check: Check, reason: impl Into<String>) -> Self {
        Self::new(check, Verdict::NotObservable, vec![reason.into()])
    }

    /// Returns the obligation.
    #[must_use]
    pub fn check(&self) -> Check {
        self.check
    }

    /// Returns the verdict.
    #[must_use]
    pub fn verdict(&self) -> Verdict {
        self.verdict
    }

    /// Returns the evidence, one line per observation.
    #[must_use]
    pub fn evidence(&self) -> &[String] {
        &self.evidence
    }
}

/// The findings of the node profile for one CDR product.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    /// The product and its release, as the report names it.
    product: String,
    /// The findings, in the order they were recorded.
    findings: Vec<Finding>,
}

impl Profile {
    /// Creates an empty profile of `product`.
    #[must_use]
    pub fn new(product: impl Into<String>) -> Self {
        Self {
            product: product.into(),
            findings: Vec::new(),
        }
    }

    /// Records `finding`.
    pub fn record(&mut self, finding: Finding) {
        self.findings.push(finding);
    }

    /// Returns the product.
    #[must_use]
    pub fn product(&self) -> &str {
        &self.product
    }

    /// Returns the findings.
    #[must_use]
    pub fn findings(&self) -> &[Finding] {
        &self.findings
    }

    /// Returns the findings as tab-separated rows under [`FINDINGS_HEADER`],
    /// one row per finding and point it assists.
    ///
    /// Tabs and line breaks inside a cell become spaces, and the evidence
    /// lines are joined with `; `.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrofed_testkit::node_profile::{Check, Finding, Profile, Verdict};
    ///
    /// let mut profile = Profile::new("Example CDR 1.0");
    /// let evidence = vec!["GET /ehr/{ehr_id} answered 200".to_owned()];
    /// profile.record(Finding::new(Check::InvocableOnEhrId, Verdict::Pass, evidence));
    /// let rows = profile.to_tsv();
    /// assert!(rows.contains("Example CDR 1.0\tCP-27\tinvocable on ehr_id alone\tpass\t"));
    /// ```
    #[must_use]
    pub fn to_tsv(&self) -> String {
        let mut lines = vec![FINDINGS_HEADER.to_owned()];
        for finding in &self.findings {
            let evidence = cell(&finding.evidence.join("; "));
            for point in finding.check.points() {
                lines.push(format!(
                    "{}\t{point}\t{}\t{}\t{evidence}",
                    cell(&self.product),
                    cell(finding.check.name()),
                    finding.verdict
                ));
            }
        }
        lines.push(String::new());
        lines.join("\n")
    }

    /// Writes [`Profile::to_tsv`] to `dir/name.tsv`, creating `dir`, and
    /// returns the path written.
    ///
    /// # Errors
    ///
    /// Returns the I/O error when the directory cannot be created or the file
    /// cannot be written.
    pub fn write(&self, dir: &Path, name: &str) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{name}.tsv"));
        std::fs::write(&path, self.to_tsv())?;
        Ok(path)
    }
}

/// Returns the directory findings are written to: the one
/// [`FINDINGS_DIR_VARIABLE`] names, else [`DEFAULT_FINDINGS_DIR`] under the
/// workspace root.
#[must_use]
pub fn findings_dir() -> PathBuf {
    std::env::var_os(FINDINGS_DIR_VARIABLE).map_or_else(
        || {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(DEFAULT_FINDINGS_DIR)
        },
        PathBuf::from,
    )
}

/// One cell of a findings row, with no tab or line break.
fn cell(text: &str) -> String {
    text.replace(['\t', '\n', '\r'], " ")
}

/// A Basic credential a check presents to the node (RFC 7617).
#[derive(Clone)]
pub struct Credential {
    /// The user name.
    user: String,
    /// The password, never shown.
    password: String,
}

impl Credential {
    /// Creates the Basic credential of `user` with `password`.
    #[must_use]
    pub fn basic(user: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            user: user.into(),
            password: password.into(),
        }
    }

    /// Returns the user name.
    #[must_use]
    pub fn user(&self) -> &str {
        &self.user
    }
}

impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credential")
            .field("user", &self.user)
            .field("password", &"[redacted]")
            .finish()
    }
}

/// A refusal the operator arranged at the node: an EHR the node holds, a
/// principal its policy serves that EHR to, and one it refuses it to.
#[derive(Debug, Clone)]
pub struct Arrangement {
    /// The EHR.
    ehr_id: Uuid,
    /// The principal the node serves the EHR to.
    permitted: Credential,
    /// The principal the node refuses it to.
    refused: Credential,
}

impl Arrangement {
    /// Creates the arrangement over `ehr_id`, served to `permitted` and
    /// refused to `refused`.
    #[must_use]
    pub fn new(ehr_id: Uuid, permitted: Credential, refused: Credential) -> Self {
        Self {
            ehr_id,
            permitted,
            refused,
        }
    }

    /// Returns the EHR the arrangement is over.
    #[must_use]
    pub fn ehr_id(&self) -> Uuid {
        self.ehr_id
    }
}

/// A check could not observe the node at all.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CheckError {
    /// The HTTP client could not be built.
    #[error("the HTTP client of the node checks could not be built")]
    Client(#[source] reqwest::Error),
    /// The query body could not be written.
    #[error("the query body could not be written")]
    Body(#[source] serde_json::Error),
    /// A request could not be sent, or its answer not read.
    #[error("{step} reached no answer")]
    Send {
        /// The request, as method and path.
        step: String,
        /// What the HTTP stack reported.
        #[source]
        source: reqwest::Error,
    },
}

/// The ITS-REST interface of one node, as the checks reach it.
#[derive(Debug, Clone)]
pub struct Interface {
    /// The API root `/v1/ehr` lives under, with no trailing slash.
    api_root: String,
    /// The client every request goes through.
    client: reqwest::Client,
    /// The credential each request carries unless a check names another.
    credential: Option<Credential>,
}

/// What the node answered one request.
#[derive(Debug)]
struct Answer {
    /// The status.
    status: http::StatusCode,
    /// The `Location` header, when there is one.
    location: Option<String>,
    /// The body.
    body: Vec<u8>,
}

impl Interface {
    /// Creates the interface at `api_root`, the URL `/v1/ehr` lives under.
    ///
    /// # Errors
    ///
    /// Returns [`CheckError::Client`] when the HTTP client cannot be built.
    pub fn new(api_root: &str) -> Result<Self, CheckError> {
        let client = reqwest::Client::builder()
            .build()
            .map_err(CheckError::Client)?;
        Ok(Self {
            api_root: api_root.trim_end_matches('/').to_owned(),
            client,
            credential: None,
        })
    }

    /// Returns the interface with every request carrying `credential`.
    #[must_use]
    pub fn with_credential(mut self, credential: Credential) -> Self {
        self.credential = Some(credential);
        self
    }

    /// Sends `GET {api_root}/v1/{path}` as `credential`, or as the
    /// interface's own credential.
    async fn get(&self, path: &str, credential: Option<&Credential>) -> Result<Answer, CheckError> {
        let request = self
            .client
            .get(format!("{}/v1/{path}", self.api_root))
            .header(ACCEPT, "application/json");
        self.send(format!("GET /{path}"), request, credential).await
    }

    /// Sends `POST {api_root}/v1/ehr` with no body, asking for the created
    /// EHR in the answer.
    async fn create_ehr(&self) -> Result<Answer, CheckError> {
        let request = self
            .client
            .post(format!("{}/v1/ehr", self.api_root))
            .header(ACCEPT, "application/json")
            .header("Prefer", "return=representation");
        self.send("POST /ehr".to_owned(), request, None).await
    }

    /// Sends `aql` as `POST {api_root}/v1/query/aql` as `credential`, or as
    /// the interface's own credential.
    async fn query(
        &self,
        aql: &str,
        credential: Option<&Credential>,
    ) -> Result<Answer, CheckError> {
        let body = openehr_its::rest::generated::query::AdhocQueryExecute {
            q: aql.to_owned(),
            offset: None,
            fetch: None,
            query_parameters: None,
            additional_properties: std::collections::BTreeMap::new(),
        };
        let request = self
            .client
            .post(format!("{}/v1/query/aql", self.api_root))
            .header(ACCEPT, "application/json")
            .header(CONTENT_TYPE, "application/json")
            .body(serde_json::to_vec(&body).map_err(CheckError::Body)?);
        self.send("POST /query/aql".to_owned(), request, credential)
            .await
    }

    /// Sends `request`, as `credential` or the interface's own, and reads the
    /// whole answer.
    async fn send(
        &self,
        step: String,
        request: reqwest::RequestBuilder,
        credential: Option<&Credential>,
    ) -> Result<Answer, CheckError> {
        let request = match credential.or(self.credential.as_ref()) {
            Some(credential) => request.basic_auth(&credential.user, Some(&credential.password)),
            None => request,
        };
        let answer = request.send().await.map_err(|source| CheckError::Send {
            step: step.clone(),
            source,
        })?;
        let status = answer.status();
        let location = answer
            .headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let body = answer
            .bytes()
            .await
            .map_err(|source| CheckError::Send { step, source })?
            .to_vec();
        Ok(Answer {
            status,
            location,
            body,
        })
    }
}
