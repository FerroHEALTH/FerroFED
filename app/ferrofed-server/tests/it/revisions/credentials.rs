// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Onward credentials and binding sections managed from the console: each
//! owned whole, its secrets held only as references, a value written to a
//! new file only the gateway reads and never answered, logged or stored;
//! an applied change awaiting its confirmation and rolled back without it,
//! also across a restart and on every replica; and a second operator's
//! approval where `config.approval` names the resource. No specification
//! governs this: our own design.

use std::error::Error;

use ferrofed_registry::console::{ConsoleState, Owner};
use ferrofed_registry::resource::{BindingSection, Resource, SecretChange, SectionRequest};
use ferrofed_registry::revision::{Applied, Author, Current, Review, Revision, RevisionId};
use ferrofed_registry::secret::WriteOnly;
use http::{Method, StatusCode};
use jiff::{SignedDuration, Timestamp};

use super::{Gateway, OPERATOR, Replicas, Standing, TestResult, operator_as};
use crate::support::error_body;

/// The subject of the second operator, who approves.
const SECOND: &str = "synthetic-config-operator-73";

/// A synthetic secret value no answer, log, store or revision may hold.
const VALUE: &str = "synthetic-onward-secret-c4a9";

/// The endpoint member A's credentials are for.
const ENDPOINT: &str = "node-a-pub";

/// The revision numbered `number`.
fn id(number: u64) -> Result<RevisionId, &'static str> {
    RevisionId::new(number).ok_or("a revision number")
}

/// The Basic password set by a reference to `file`.
fn by_file(file: &str) -> SecretChange {
    SecretChange {
        key: String::from("password"),
        value: None,
        file: Some(file.to_owned()),
        clear: false,
    }
}

/// The Basic password set by the value [`VALUE`].
fn by_value() -> SecretChange {
    SecretChange {
        key: String::from("password"),
        value: Some(WriteOnly::new(VALUE)),
        file: None,
        clear: false,
    }
}

/// The body of a binding section's save on `base` with `settings`.
fn section(base: u64, settings: &str) -> Result<String, Box<dyn Error>> {
    Ok(serde_json::to_string(&SectionRequest {
        base: Some(id(base)?),
        reason: String::from("the deployment's choice"),
        settings: settings.to_owned(),
        secrets: Vec::new(),
    })?)
}

/// `[config]` keys with a secrets directory at `dir`.
fn with_secrets_dir(dir: &std::path::Path, extra: &str) -> String {
    let dir = toml::Value::String(dir.display().to_string());
    format!("secrets_dir = {dir}\n{extra}")
}

impl Gateway {
    /// Sends `method` of `path` with the JSON `body` as the operator
    /// `subject`.
    async fn as_operator(
        &self,
        subject: &str,
        (method, path): (Method, &str),
        body: Option<String>,
    ) -> Result<(StatusCode, String), Box<dyn Error>> {
        self.send((method, path), body, Some(&operator_as(subject)?))
            .await
    }

    /// Saves the credentials of [`ENDPOINT`] on `base` with `secrets`, as
    /// the first operator.
    async fn save_credentials(
        &self,
        base: u64,
        settings: &str,
        secrets: Vec<SecretChange>,
    ) -> Result<(StatusCode, String), Box<dyn Error>> {
        let body = serde_json::to_string(&SectionRequest {
            base: Some(id(base)?),
            reason: String::from("the node's token"),
            settings: settings.to_owned(),
            secrets,
        })?;
        self.as_operator(
            OPERATOR,
            (
                Method::PUT,
                &format!("/admin/config/credentials/{ENDPOINT}"),
            ),
            Some(body),
        )
        .await
    }

    /// The store's running mark.
    fn mark(&self) -> Result<Current, Box<dyn Error>> {
        Ok(self.revisions.store().current()?.ok_or("a revision runs")?)
    }

    /// A token file with synthetic content, as an operator places one.
    fn token_file(&self, name: &str) -> Result<String, Box<dyn Error>> {
        let path = self.dir.path().join(name);
        std::fs::write(&path, "synthetic-referenced-token")?;
        Ok(path.display().to_string())
    }
}

/// Saves revision 2 with a Basic password of [`ENDPOINT`] by reference and
/// applies it, as the first operator.
async fn applied_credentials(gateway: &Gateway) -> Result<Applied, Box<dyn Error>> {
    let file = gateway.token_file("node-a-token")?;
    let (status, text) = gateway
        .save_credentials(1, "user = \"gateway\"", vec![by_file(&file)])
        .await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let (status, text) = gateway
        .post("/admin/config/revisions/2/apply", None)
        .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    Ok(serde_json::from_str(&text)?)
}

#[tokio::test]
async fn a_credential_change_awaits_its_confirmation_and_stays_once_confirmed() -> TestResult {
    let gateway = Gateway::configured(("confirm_ms = 60000\n", ""), None)?;
    let applied = applied_credentials(&gateway).await?;
    assert_eq!(
        vec![Resource::Credentials(ENDPOINT.to_owned())],
        applied.revision.console
    );
    let awaiting = applied.current.awaiting.ok_or("the apply awaits")?;
    assert_eq!(vec!["[credentials.\"node-a-pub\"]"], awaiting.resources);
    let (_, text) = gateway.get("/admin/config").await?;
    let standing: Standing = serde_json::from_str(&text)?;
    assert!(standing.current.and_then(|mark| mark.awaiting).is_some());
    let (status, text) = gateway
        .post("/admin/config/revisions/2/confirm", None)
        .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let confirmed: Current = serde_json::from_str(&text)?;
    assert_eq!(None, confirmed.awaiting);
    assert!(matches!(
        confirmed.confirmed.map(|signed| signed.by),
        Some(Author::Operator { subject, .. }) if subject == OPERATOR
    ));
    let later = Timestamp::now().checked_add(SignedDuration::from_hours(2))?;
    assert!(
        gateway
            .revisions
            .expire(&gateway.reloader, later)?
            .is_none()
    );
    assert_eq!(id(2)?, gateway.mark()?.revision);
    let (status, text) = gateway
        .post("/admin/config/revisions/2/confirm", None)
        .await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    assert_eq!("confirmation-unavailable", error_body(&text)?.code);
    Ok(())
}

#[tokio::test]
async fn an_unconfirmed_change_rolls_back_at_its_deadline() -> TestResult {
    let gateway = Gateway::configured(("confirm_ms = 60000\n", ""), None)?;
    let applied = applied_credentials(&gateway).await?;
    let deadline = applied.current.awaiting.ok_or("the apply awaits")?.deadline;
    let before = deadline.checked_sub(SignedDuration::from_secs(1))?;
    assert!(
        gateway
            .revisions
            .expire(&gateway.reloader, before)?
            .is_none()
    );
    let after = deadline.checked_add(SignedDuration::from_millis(1))?;
    let (revision, mark) = gateway
        .revisions
        .expire(&gateway.reloader, after)?
        .ok_or("the deadline passed")?;
    assert_eq!(Author::Gateway, revision.author);
    assert_eq!(Some(id(1)?), revision.restores);
    assert!(
        revision.reason.contains("not confirmed"),
        "{}",
        revision.reason
    );
    assert_eq!(revision.id, mark.revision);
    assert_eq!(None, mark.awaiting);
    assert_eq!(id(3)?, gateway.mark()?.revision);
    assert_eq!(revision.console, Vec::<Resource>::new());
    let (status, text) = gateway
        .post("/admin/config/revisions/2/confirm", None)
        .await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    assert_eq!("confirmation-unavailable", error_body(&text)?.code);
    Ok(())
}

#[tokio::test]
async fn a_restart_rolls_back_a_change_past_its_deadline() -> TestResult {
    let gateway = Gateway::configured(("confirm_ms = 60000\n", ""), None)?;
    let applied = applied_credentials(&gateway).await?;
    let mut lapsed = applied.current;
    if let Some(awaiting) = &mut lapsed.awaiting {
        awaiting.deadline = Timestamp::now().checked_sub(SignedDuration::from_secs(1))?;
    }
    let swapped = gateway
        .revisions
        .store()
        .swap_current(Some(id(2)?), &lapsed)?;
    assert_eq!(ferrofed_server::revision::Swapped::Swapped, swapped);
    let gateway = gateway.restart()?;
    let mark = gateway.mark()?;
    assert_eq!(id(3)?, mark.revision);
    assert_eq!(Author::Gateway, mark.applied_by);
    let rolled: Revision = gateway
        .revisions
        .store()
        .revision(id(3)?)?
        .ok_or("the rollback is held")?;
    assert_eq!(Some(id(1)?), rolled.restores);
    assert!(!rolled.toml.contains("password_file"), "{}", rolled.toml);
    Ok(())
}

#[tokio::test]
async fn a_restart_within_the_deadline_keeps_awaiting() -> TestResult {
    let gateway = Gateway::configured(("confirm_ms = 60000\n", ""), None)?;
    applied_credentials(&gateway).await?;
    let gateway = gateway.restart()?;
    let mark = gateway.mark()?;
    assert_eq!(id(2)?, mark.revision);
    assert!(mark.awaiting.is_some());
    Ok(())
}

#[tokio::test]
async fn another_apply_waits_for_the_confirmation() -> TestResult {
    let gateway = Gateway::start("")?;
    applied_credentials(&gateway).await?;
    let body = section(2, "disclose = false")?;
    let (status, text) = gateway
        .as_operator(
            OPERATOR,
            (Method::PUT, "/admin/config/bindings/consent"),
            Some(body),
        )
        .await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let (status, text) = gateway
        .post("/admin/config/revisions/3/apply", None)
        .await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    assert_eq!("confirmation-pending", error_body(&text)?.code);
    Ok(())
}

#[tokio::test]
async fn an_unconfirmed_change_rolled_back_on_one_replica_reaches_the_other() -> TestResult {
    let replicas = Replicas::start()?;
    applied_credentials(&replicas.one).await?;
    let followed = replicas.two.revisions.follow(&replicas.two.reloader)?;
    assert_eq!(
        ferrofed_server::revision::service::Followed::Applied(id(2)?),
        followed
    );
    let later = Timestamp::now().checked_add(SignedDuration::from_hours(1))?;
    let (revision, _) = replicas
        .one
        .revisions
        .expire(&replicas.one.reloader, later)?
        .ok_or("the deadline passed")?;
    assert!(
        replicas
            .two
            .revisions
            .expire(&replicas.two.reloader, later)?
            .is_none(),
        "one replica rolls it back"
    );
    let followed = replicas.two.revisions.follow(&replicas.two.reloader)?;
    assert_eq!(
        ferrofed_server::revision::service::Followed::Applied(revision.id),
        followed
    );
    Ok(())
}

#[tokio::test]
async fn approval_by_the_author_is_refused_and_by_a_second_operator_admits_the_apply() -> TestResult
{
    let gateway = Gateway::configured(("approval = [\"onward-credentials\"]\n", ""), None)?;
    let file = gateway.token_file("node-a-token")?;
    let (status, text) = gateway
        .save_credentials(1, "user = \"gateway\"", vec![by_file(&file)])
        .await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let (status, text) = gateway
        .post("/admin/config/revisions/2/apply", None)
        .await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    assert_eq!("approval-required", error_body(&text)?.code);
    let (_, text) = gateway.get("/admin/config").await?;
    let standing: Standing = serde_json::from_str(&text)?;
    let review = standing.review.ok_or("the newest revision is reviewed")?;
    assert_eq!(vec!["[credentials.\"node-a-pub\"]"], review.approval);
    assert!(!review.approved);
    let approve = (Method::POST, "/admin/config/revisions/2/approve");
    let (status, text) = gateway.as_operator(OPERATOR, approve.clone(), None).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "{text}");
    assert_eq!("approval-own-revision", error_body(&text)?.code);
    let (status, text) = gateway.as_operator(SECOND, approve, None).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let review: Review = serde_json::from_str(&text)?;
    assert!(review.approved);
    assert!(matches!(
        &*review.approvals,
        [signed] if matches!(&signed.by, Author::Operator { subject, .. } if subject == SECOND)
    ));
    assert!(matches!(&review.author, Author::Operator { subject, .. } if subject == OPERATOR));
    let (status, text) = gateway
        .post("/admin/config/revisions/2/apply", None)
        .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    Ok(())
}

#[tokio::test]
async fn an_approval_of_a_revision_that_needs_none_is_refused() -> TestResult {
    let gateway = Gateway::configured(("approval = [\"consent\"]\n", ""), None)?;
    let file = gateway.token_file("node-a-token")?;
    let (status, text) = gateway
        .save_credentials(1, "user = \"gateway\"", vec![by_file(&file)])
        .await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let (status, text) = gateway
        .as_operator(
            SECOND,
            (Method::POST, "/admin/config/revisions/2/approve"),
            None,
        )
        .await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    assert_eq!("approval-not-required", error_body(&text)?.code);
    Ok(())
}

#[tokio::test]
async fn a_secret_value_is_written_to_an_owner_only_file_and_never_shown() -> TestResult {
    let secrets = tempfile::tempdir()?;
    let logs = crate::support::Logs::default();
    let capture = ferrofed_server::telemetry::subscriber(
        ferrofed_server::telemetry::Rendering::Json,
        "trace",
        false,
        logs.clone(),
    )?;
    tracing::subscriber::set_global_default(capture)?;
    let gateway = Gateway::configured((&with_secrets_dir(secrets.path(), ""), ""), None)?;
    let (status, text) = gateway
        .save_credentials(1, "user = \"gateway\"", vec![by_value()])
        .await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    assert!(!text.contains(VALUE), "{text}");
    let saved: Revision = serde_json::from_str(&text)?;
    let held: toml::Table = toml::from_str(&saved.toml)?;
    let reference = held["credentials"][ENDPOINT]["password_file"]
        .as_str()
        .ok_or("the revision references the file")?;
    assert!(reference.starts_with(&secrets.path().display().to_string()));
    assert_eq!(VALUE, std::fs::read_to_string(reference)?);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(
            0o600,
            std::fs::metadata(reference)?.permissions().mode() & 0o777
        );
    }
    let mut answers = Vec::new();
    for (method, path) in [
        (Method::POST, "/admin/config/revisions/2/apply"),
        (Method::GET, "/admin/config"),
        (Method::GET, "/admin/config/revisions"),
        (Method::GET, "/admin/config/revisions/2"),
        (Method::GET, "/admin/config/revisions/1/diff/2"),
        (Method::GET, "/admin/config/resources"),
        (Method::POST, "/admin/config/revisions/2/confirm"),
    ] {
        let (status, text) = gateway.as_operator(OPERATOR, (method, path), None).await?;
        assert_eq!(StatusCode::OK, status, "{path}: {text}");
        answers.push(text);
    }
    for answer in &answers {
        assert!(!answer.contains(VALUE), "{answer}");
    }
    let (_, resources) = gateway.get("/admin/config/resources").await?;
    let resources: ferrofed_registry::console::Resources = serde_json::from_str(&resources)?;
    let section = resources
        .sections
        .iter()
        .find(|section| section.resource == Resource::Credentials(ENDPOINT.to_owned()))
        .ok_or("the credential is listed")?;
    assert_eq!(Owner::Console, section.owner);
    assert_eq!(Some(ConsoleState::Running), section.state);
    assert_eq!(
        Some(String::from("{ user = \"gateway\" }")),
        section.settings
    );
    assert_eq!("password", section.secrets[0].key);
    assert_eq!(reference, section.secrets[0].reference);
    let store = std::fs::read(&gateway.store)?;
    assert!(
        !store
            .windows(VALUE.len())
            .any(|window| window == VALUE.as_bytes()),
        "the store holds the reference alone"
    );
    let text = logs.text();
    assert!(text.contains("config-revision-saved"), "{text}");
    assert!(!text.contains(VALUE), "{text}");
    Ok(())
}

#[tokio::test]
async fn a_secret_value_without_a_secrets_directory_is_refused() -> TestResult {
    let gateway = Gateway::start("")?;
    let (status, text) = gateway
        .save_credentials(1, "user = \"gateway\"", vec![by_value()])
        .await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    assert_eq!("secret-store-unavailable", error_body(&text)?.code);
    assert!(!text.contains(VALUE), "{text}");
    assert_eq!(Some(id(1)?), gateway.revisions.store().head()?);
    Ok(())
}

#[tokio::test]
async fn a_refused_save_removes_the_secret_file_it_wrote() -> TestResult {
    let secrets = tempfile::tempdir()?;
    let gateway = Gateway::configured((&with_secrets_dir(secrets.path(), ""), ""), None)?;
    let (status, text) = gateway
        .save_credentials(1, "user = \"gate:way\"", vec![by_value()])
        .await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
    assert!(!text.contains(VALUE), "{text}");
    assert_eq!(0, std::fs::read_dir(secrets.path())?.count());
    Ok(())
}

// The gateway refuses a token endpoint with a query, so the revision is put
// in the store directly: the answer redacts whatever a revision holds.
#[tokio::test]
async fn a_credential_in_a_url_setting_never_reaches_the_resources_answer() -> TestResult {
    let gateway = Gateway::start("")?;
    let base = gateway
        .revisions
        .store()
        .revision(id(1)?)?
        .ok_or("the file's revision is held")?;
    let toml = format!(
        "{}\n[credentials.\"{ENDPOINT}\".oauth2]\ntoken_endpoint = \"https://u:{VALUE}@as.example.org/token?api_key={VALUE}\"\nclient_id = \"ferrofed\"\n",
        base.toml
    );
    let draft = ferrofed_server::revision::Draft {
        origin: ferrofed_registry::revision::Origin::Operator,
        restores: None,
        author: operator_author(),
        at: Timestamp::now(),
        reason: String::from("synthetic"),
        toml,
        console: vec![Resource::Credentials(ENDPOINT.to_owned())],
    };
    let saved = gateway.revisions.store().save(Some(id(1)?), draft)?;
    assert!(matches!(saved, ferrofed_server::revision::Saved::Stored(_)));
    let (status, text) = gateway.get("/admin/config/resources").await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert!(!text.contains(VALUE), "{text}");
    assert!(text.contains("as.example.org/token?***"), "{text}");
    Ok(())
}

#[tokio::test]
async fn settings_that_name_a_secret_are_refused() -> TestResult {
    let gateway = Gateway::start("")?;
    let (status, text) = gateway
        .save_credentials(
            1,
            &format!("user = \"gateway\"\npassword = \"{VALUE}\""),
            Vec::new(),
        )
        .await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
    assert_eq!("body-invalid", error_body(&text)?.code);
    assert!(!text.contains(VALUE), "{text}");
    Ok(())
}

#[tokio::test]
async fn a_credential_the_file_provides_is_read_only() -> TestResult {
    let secrets = tempfile::tempdir()?;
    let password = secrets.path().join("node-a");
    std::fs::write(&password, "synthetic-file-password")?;
    let reference = password.display().to_string();
    let gateway = Gateway::start(&format!(
        "\n[credentials.\"node-a-pub\"]\nuser = \"synthetic-user\"\npassword_file = {}\n",
        toml::Value::String(reference.clone())
    ))?;
    let (status, text) = gateway
        .save_credentials(1, "user = \"other\"", Vec::new())
        .await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    assert_eq!("resource-file-owned", error_body(&text)?.code);
    let (status, text) = gateway
        .as_operator(
            OPERATOR,
            (Method::POST, "/admin/config/credentials/node-a-pub/remove"),
            Some(String::from(r#"{"base": 1, "reason": "remove"}"#)),
        )
        .await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    assert_eq!("resource-file-owned", error_body(&text)?.code);
    let (_, text) = gateway.get("/admin/config/resources").await?;
    let resources: ferrofed_registry::console::Resources = serde_json::from_str(&text)?;
    let section = resources
        .sections
        .iter()
        .find(|section| section.resource == Resource::Credentials(ENDPOINT.to_owned()))
        .ok_or("the file's credential is listed")?;
    assert_eq!(Owner::File, section.owner);
    assert_eq!(None, section.settings);
    assert_eq!(reference, section.secrets[0].reference);
    Ok(())
}

#[tokio::test]
async fn a_consent_change_applies_through_the_reload_and_survives_a_file_reload() -> TestResult {
    let gateway = Gateway::start("")?;
    let discloses = |gateway: &Gateway| {
        gateway
            .state
            .federation()
            .map(|federation| federation.deployment_discloses_consent())
    };
    assert_eq!(Some(true), discloses(&gateway));
    let body = section(1, "{ disclose = false }")?;
    let (status, text) = gateway
        .as_operator(
            OPERATOR,
            (Method::PUT, "/admin/config/bindings/consent"),
            Some(body),
        )
        .await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let (status, text) = gateway
        .post("/admin/config/revisions/2/apply", None)
        .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(Some(false), discloses(&gateway));
    let (status, text) = gateway
        .post("/admin/config/revisions/2/confirm", None)
        .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let file = std::fs::read_to_string(&gateway.config)?;
    std::fs::write(
        &gateway.config,
        file.replace("per_node_timeout_ms = 5000", "per_node_timeout_ms = 4000"),
    )?;
    gateway.reloader.reload()?;
    let mark = gateway.mark()?;
    assert_eq!(id(3)?, mark.revision);
    let recorded = gateway
        .revisions
        .store()
        .revision(id(3)?)?
        .ok_or("the file's revision is held")?;
    assert_eq!(
        vec![Resource::Binding(BindingSection::Consent)],
        recorded.console
    );
    assert_eq!(Some(false), discloses(&gateway));
    Ok(())
}

#[tokio::test]
async fn a_binding_section_the_file_comes_to_provide_is_the_files() -> TestResult {
    let gateway = Gateway::start("")?;
    let body = section(1, "disclose = false")?;
    let (status, text) = gateway
        .as_operator(
            OPERATOR,
            (Method::PUT, "/admin/config/bindings/consent"),
            Some(body),
        )
        .await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let (status, text) = gateway
        .post("/admin/config/revisions/2/apply", None)
        .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    gateway
        .revisions
        .confirm(id(2)?, operator_author(), Timestamp::now())?;
    let file = std::fs::read_to_string(&gateway.config)?;
    std::fs::write(
        &gateway.config,
        file + "\n[federation.consent]\ndisclose = true\n",
    )?;
    gateway.reloader.reload()?;
    let recorded = gateway
        .revisions
        .store()
        .revision(gateway.mark()?.revision)?
        .ok_or("the file's revision is held")?;
    assert_eq!(recorded.console, Vec::<Resource>::new());
    assert_eq!(
        vec![Resource::Binding(BindingSection::Consent)],
        gateway.revisions.file_owned()?
    );
    let body = section(recorded.id.get(), "disclose = false")?;
    let (status, text) = gateway
        .as_operator(
            OPERATOR,
            (Method::PUT, "/admin/config/bindings/consent"),
            Some(body),
        )
        .await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    assert_eq!("resource-file-owned", error_body(&text)?.code);
    Ok(())
}

#[tokio::test]
async fn a_confirmation_needs_a_verified_operator_and_a_binding_this_build_lacks_is_unknown()
-> TestResult {
    let gateway = Gateway::start("")?;
    applied_credentials(&gateway).await?;
    let refused = gateway
        .revisions
        .confirm(id(2)?, Author::DevelopmentLoopback, Timestamp::now());
    assert!(matches!(
        refused,
        Err(ferrofed_server::revision::service::RevisionError::Unverified)
    ));
    let (status, text) = gateway
        .as_operator(
            OPERATOR,
            (Method::PUT, "/admin/config/bindings/auth"),
            Some(section(2, "")?),
        )
        .await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
    assert_eq!("resource-unknown", error_body(&text)?.code);
    Ok(())
}

/// The first operator, as a revision records an author.
fn operator_author() -> Author {
    Author::Operator {
        issuer: String::from(crate::support::ISSUER),
        subject: String::from(OPERATOR),
    }
}
