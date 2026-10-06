// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Keycloak in the harness: the identity provider the production guide's
//! issuer recipe is written for, behind the same gate.
//!
//! [`keycloak`] starts the pinned [`KEYCLOAK`] image in its development mode
//! with a bootstrap administrator, and [`Keycloak::apply`] runs a
//! [`recipe::Recipe`] against it with Keycloak's own admin CLI, `kcadm.sh`.
//! The Keycloak image carries no `awk`, which the recipe's `scope_of`
//! function pipes into, so the commands run in a second container on the
//! pinned [`TEMURIN_JRE`] image, a Java runtime on Ubuntu, with the admin
//! CLI copied out of the Keycloak container. The recipe's commands and files
//! reach that container as the page prints them, with only the address
//! `kcadm.sh` logs in to replaced.
//!
//! [`Keycloak::client_credentials_token`] and [`Keycloak::user_token`] then
//! mint the two tokens the recipe promises: a reporting service's by the
//! client-credentials grant (RFC 6749 §4.4) and a user's by the
//! authorization code grant (RFC 6749 §4.1), the user signing in through
//! Keycloak's login form as a browser would. Every user, password and
//! secret is a synthetic development value. Keycloak is an issuer here and
//! never the oracle. No specification governs which products the harness
//! runs: our own design.

use std::time::Duration;

use http::StatusCode;
use http::header::{CONTENT_TYPE, COOKIE, LOCATION, SET_COOKIE};
use serde::Deserialize;
use testcontainers::core::{CmdWaitFor, ExecCommand, IntoContainerPort};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, CopyTargetOptions, GenericImage, ImageExt};
use url::Url;

pub mod recipe;

use super::{HarnessError, KEYCLOAK, TEMURIN_JRE, await_readiness, names};
use recipe::{Recipe, RecipeError};

/// The port Keycloak serves HTTP on inside its container.
const PORT: u16 = 8080;

/// The bootstrap administrator of the `master` realm, the user the recipe's
/// `kcadm.sh config credentials --user admin` logs in as.
const ADMIN_USER: &str = "admin";

/// The bootstrap administrator's development password.
const ADMIN_PASSWORD: &str = "keycloak-admin-example";

/// The realm the recipe creates.
pub const REALM: &str = "ferrofed";

/// The clinical application the recipe registers for users who sign in.
pub const CLINICAL_APP: &str = "example-clinical-app";

/// The redirect URI the recipe registers for [`CLINICAL_APP`].
const CLINICAL_REDIRECT: &str = "https://app.example.org/callback";

/// The reporting service the recipe registers for the client-credentials
/// grant.
pub const REPORTING_SERVICE: &str = "example-reporting-service";

/// The synthetic user the harness signs in to [`CLINICAL_APP`] as.
const USER: &str = "example-clinician";

/// The synthetic user's development password.
const USER_PASSWORD: &str = "example-clinician-password";

/// Where the admin CLI sits in the Keycloak image, and where the runner
/// gets a copy of it.
const ADMIN_CLI: &str = "/opt/keycloak/bin";

/// The directory the runner holds the recipe's commands and files in, its
/// working directory.
const WORK: &str = "/recipe";

/// How long one `kcadm.sh` script may run.
const SCRIPT_BUDGET: Duration = Duration::from_secs(300);

/// What the harness could not do with Keycloak.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum KeycloakError {
    /// A container could not be started.
    #[error(transparent)]
    Harness(#[from] HarnessError),
    /// The recipe could not be read from the page, or applied to Keycloak.
    #[error(transparent)]
    Recipe(#[from] RecipeError),
    /// Docker refused to run a command in, or copy a file out of, a
    /// container.
    #[error("{step} failed in its container")]
    Docker {
        /// What the harness was doing.
        step: &'static str,
        /// What Docker reported.
        #[source]
        source: testcontainers::TestcontainersError,
    },
    /// A `kcadm.sh` script exited with a failure; its output is attached.
    #[error("{step} exited with {code:?}:\n{output}")]
    Script {
        /// The script.
        step: &'static str,
        /// Its exit code.
        code: Option<i64>,
        /// What it wrote to standard output and standard error.
        output: String,
    },
    /// A request to Keycloak could not be sent or its answer read.
    #[error("{step} could not be sent or read")]
    Exchange {
        /// The step.
        step: &'static str,
        /// What the HTTP stack reported.
        #[source]
        source: reqwest::Error,
    },
    /// Keycloak answered a step in a way the harness cannot continue from.
    #[error("{step} was answered {status}: {detail}")]
    Answer {
        /// The step.
        step: &'static str,
        /// The status Keycloak answered.
        status: StatusCode,
        /// What was missing from the answer.
        detail: String,
    },
}

/// A started Keycloak, with the container the recipe's commands run in,
/// torn down when it is dropped.
#[derive(Debug)]
pub struct Keycloak {
    /// Keycloak itself.
    server: ContainerAsync<GenericImage>,
    /// The container `kcadm.sh` runs in, once [`Keycloak::apply`] started it.
    runner: Option<ContainerAsync<GenericImage>>,
    /// The network both share.
    network: String,
    /// The container name the runner reaches Keycloak by.
    host: String,
    /// The origin the host reaches Keycloak at, with no path.
    origin: String,
}

/// Starts the pinned Keycloak in its development mode and waits until it
/// serves the `master` realm.
///
/// # Errors
///
/// Returns [`HarnessError::Container`] when Docker refuses the container and
/// [`HarnessError::NotReady`] when Keycloak does not serve in time.
pub async fn keycloak() -> Result<Keycloak, KeycloakError> {
    let (network, host) = names("keycloak");
    let container = |source| HarnessError::Container {
        image: KEYCLOAK.repository,
        source,
    };
    let server = KEYCLOAK
        .image()
        .with_exposed_port(PORT.tcp())
        .with_cmd(["start-dev"])
        .with_env_var("KC_BOOTSTRAP_ADMIN_USERNAME", ADMIN_USER)
        .with_env_var("KC_BOOTSTRAP_ADMIN_PASSWORD", ADMIN_PASSWORD)
        .with_network(network.clone())
        .with_container_name(host.clone())
        .start()
        .await
        .map_err(container)?;
    let address = server.get_host().await.map_err(container)?;
    let port = server
        .get_host_port_ipv4(PORT.tcp())
        .await
        .map_err(container)?;
    let origin = format!("http://{address}:{port}");
    await_readiness(&format!("{origin}/realms/master")).await?;
    Ok(Keycloak {
        server,
        runner: None,
        network,
        host,
        origin,
    })
}

/// The answer of Keycloak's token endpoint the harness reads.
#[derive(Debug, Deserialize)]
struct TokenAnswer {
    access_token: String,
}

/// The secrets Keycloak generated for the recipe's two clients.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Secrets {
    /// The secret of [`CLINICAL_APP`].
    pub clinical_app: String,
    /// The secret of [`REPORTING_SERVICE`].
    pub reporting_service: String,
}

impl Keycloak {
    /// Returns the origin the host reaches Keycloak at, with no path: the
    /// address that takes the place of the page's `https://idp.example.org`.
    #[must_use]
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// Returns the issuer of the recipe's realm, as Keycloak writes it into
    /// a token the host asked for.
    #[must_use]
    pub fn issuer(&self) -> String {
        format!("{}/realms/{REALM}", self.origin)
    }

    /// Applies `recipe` as the page prints it, adds a synthetic user to sign
    /// in with, and returns the secrets Keycloak generated for the recipe's
    /// two clients.
    ///
    /// # Errors
    ///
    /// Returns [`KeycloakError::Docker`] when the admin CLI cannot be copied
    /// or run, [`KeycloakError::Script`] when a command of the recipe or of
    /// the harness fails, and [`KeycloakError::Recipe`] when the recipe does
    /// not name its server once.
    pub async fn apply(&mut self, recipe: &Recipe) -> Result<Secrets, KeycloakError> {
        let server = format!("http://{}:{PORT}", self.host);
        let commands = recipe.commands_against(&server)?;
        let mut runner = TEMURIN_JRE
            .image()
            .with_entrypoint("sleep")
            .with_cmd(["infinity"])
            .with_network(self.network.clone())
            .with_copy_to(
                CopyTargetOptions::new(format!("{WORK}/recipe.sh")),
                script(&commands).into_bytes(),
            )
            .with_copy_to(
                CopyTargetOptions::new(format!("{WORK}/harness.sh")),
                script(&harness_steps()).into_bytes(),
            );
        for file in recipe.files() {
            runner = runner.with_copy_to(
                CopyTargetOptions::new(format!("{WORK}/{}", file.name)),
                file.content.clone().into_bytes(),
            );
        }
        for path in self.admin_cli().await? {
            let bytes = self
                .server
                .copy_file_from(path.clone(), Vec::new())
                .await
                .map_err(|source| KeycloakError::Docker {
                    step: "copying the admin CLI out of Keycloak",
                    source,
                })?;
            runner = runner.with_copy_to(CopyTargetOptions::new(path).with_mode(0o755), bytes);
        }
        let runner = runner
            .start()
            .await
            .map_err(|source| HarnessError::Container {
                image: TEMURIN_JRE.repository,
                source,
            })?;
        run(&runner, "the recipe", "recipe.sh").await?;
        run(&runner, "the harness user and secrets", "harness.sh").await?;
        let secret = async |name: &str| -> Result<String, KeycloakError> {
            let bytes = runner
                .copy_file_from(format!("{WORK}/{name}"), Vec::new())
                .await
                .map_err(|source| KeycloakError::Docker {
                    step: "reading a client secret",
                    source,
                })?;
            Ok(String::from_utf8_lossy(&bytes).trim().to_owned())
        };
        let secrets = Secrets {
            clinical_app: secret("clinical-app.secret").await?,
            reporting_service: secret("reporting-service.secret").await?,
        };
        self.runner = Some(runner);
        Ok(secrets)
    }

    /// Sets the recipe's `access.token.header.type.rfc9068` attribute of
    /// `client` to `typed`, which decides whether Keycloak types the
    /// client's access tokens `at+jwt` (RFC 9068 §2.1).
    ///
    /// # Errors
    ///
    /// Returns [`KeycloakError::Script`] when `kcadm.sh` refuses the update,
    /// and [`KeycloakError::Answer`] when no recipe was applied.
    pub async fn type_tokens(&self, client: &str, typed: bool) -> Result<(), KeycloakError> {
        let runner = self.runner.as_ref().ok_or(KeycloakError::Answer {
            step: "typing a client's tokens",
            status: StatusCode::PRECONDITION_REQUIRED,
            detail: "no recipe was applied".to_owned(),
        })?;
        let line = format!(
            "id=\"$(kcadm.sh get clients -r {REALM} -q clientId={client} --fields id --format csv --noquotes)\"\nkcadm.sh update \"clients/$id\" -r {REALM} -s 'attributes.\"access.token.header.type.rfc9068\"={typed}'\n"
        );
        exec(runner, "typing a client's tokens", script(&line)).await
    }

    /// Returns an access token of `client` by the client-credentials grant
    /// (RFC 6749 §4.4).
    ///
    /// # Errors
    ///
    /// Returns [`KeycloakError::Exchange`] or [`KeycloakError::Answer`] when
    /// Keycloak issues no token.
    pub async fn client_credentials_token(
        &self,
        client: &str,
        secret: &str,
    ) -> Result<String, KeycloakError> {
        token(
            &self.token_endpoint(),
            &[
                ("grant_type", "client_credentials"),
                ("client_id", client),
                ("client_secret", secret),
            ],
        )
        .await
    }

    /// Returns an access token of the synthetic user signed in to
    /// [`CLINICAL_APP`] through Keycloak's login form, by the authorization
    /// code grant (RFC 6749 §4.1).
    ///
    /// # Errors
    ///
    /// Returns [`KeycloakError::Exchange`] or [`KeycloakError::Answer`] when
    /// a step of the sign-in or the token request fails.
    pub async fn user_token(&self, secret: &str) -> Result<String, KeycloakError> {
        let step = |step| move |source| KeycloakError::Exchange { step, source };
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(step("building the browser"))?;
        let mut authorize = Url::parse(&format!("{}/protocol/openid-connect/auth", self.issuer()))
            .map_err(|error| answer("the authorization request", StatusCode::OK, &error))?;
        authorize
            .query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", CLINICAL_APP)
            .append_pair("redirect_uri", CLINICAL_REDIRECT)
            .append_pair("state", "harness");
        let login = client
            .get(authorize)
            .send()
            .await
            .map_err(step("the authorization request"))?;
        let status = login.status();
        let cookies = cookies(login.headers());
        let page = login.text().await.map_err(step("the login form"))?;
        let action = form_action(&page)
            .ok_or_else(|| answer("the login form", status, &"no form action"))?;
        let signed_in = client
            .post(action)
            .header(COOKIE, cookies)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(form(&[("username", USER), ("password", USER_PASSWORD)]))
            .send()
            .await
            .map_err(step("signing in"))?;
        let status = signed_in.status();
        let code = signed_in
            .headers()
            .get(LOCATION)
            .and_then(|location| location.to_str().ok())
            .and_then(|location| Url::parse(location).ok())
            .and_then(|location| {
                location
                    .query_pairs()
                    .find(|(name, _)| name == "code")
                    .map(|(_, code)| code.into_owned())
            })
            .ok_or_else(|| answer("signing in", status, &"no code in the redirect"))?;
        token(
            &self.token_endpoint(),
            &[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("redirect_uri", CLINICAL_REDIRECT),
                ("client_id", CLINICAL_APP),
                ("client_secret", secret),
            ],
        )
        .await
    }

    /// Returns the realm's token endpoint.
    fn token_endpoint(&self) -> String {
        format!("{}/protocol/openid-connect/token", self.issuer())
    }

    /// Returns the paths of the admin CLI's script and jars in the Keycloak
    /// container.
    async fn admin_cli(&self) -> Result<Vec<String>, KeycloakError> {
        let step = "listing the admin CLI";
        let mut listed = self
            .server
            .exec(
                ExecCommand::new([
                    "find", ADMIN_CLI, "-name", "kcadm.sh", "-o", "-name", "*.jar",
                ])
                .with_cmd_ready_condition(CmdWaitFor::exit()),
            )
            .await
            .map_err(|source| KeycloakError::Docker { step, source })?;
        let stdout = listed
            .stdout_to_vec()
            .await
            .map_err(|source| KeycloakError::Docker { step, source })?;
        let paths: Vec<String> = String::from_utf8_lossy(&stdout)
            .lines()
            .map(str::to_owned)
            .collect();
        if paths.iter().any(|path| path.ends_with("/kcadm.sh")) {
            Ok(paths)
        } else {
            Err(answer(step, StatusCode::OK, &"no kcadm.sh in the image"))
        }
    }
}

/// `commands` as a script the runner executes: strict mode, the admin CLI on
/// the path, the administrator's password where `kcadm.sh` reads it, and the
/// working directory that holds the recipe's files.
fn script(commands: &str) -> String {
    format!(
        "set -euo pipefail\nexport PATH=\"{ADMIN_CLI}:$PATH\" KC_CLI_PASSWORD='{ADMIN_PASSWORD}'\ncd {WORK}\n{commands}"
    )
}

/// The harness's own steps after the recipe, never the page's: a synthetic
/// user to sign in as, and each client's generated secret in a file.
fn harness_steps() -> String {
    let secret = |client: &str, file: &str| {
        format!(
            "id=\"$(kcadm.sh get clients -r {REALM} -q clientId={client} --fields id --format csv --noquotes)\"\nkcadm.sh get \"clients/$id/client-secret\" -r {REALM} --fields value --format csv --noquotes > {file}\n"
        )
    };
    format!(
        "kcadm.sh create users -r {REALM} -s username={USER} -s enabled=true -s emailVerified=true -s email={USER}@example.org -s firstName=Example -s lastName=Clinician\nkcadm.sh set-password -r {REALM} --username {USER} --new-password '{USER_PASSWORD}'\n{}{}",
        secret(CLINICAL_APP, "clinical-app.secret"),
        secret(REPORTING_SERVICE, "reporting-service.secret")
    )
}

/// Runs the script `name` of the runner's working directory.
async fn run(
    runner: &ContainerAsync<GenericImage>,
    step: &'static str,
    name: &str,
) -> Result<(), KeycloakError> {
    exec(runner, step, format!("bash {WORK}/{name}")).await
}

/// Runs `text` with `bash` in `runner` and fails with its output unless it
/// exits `0`.
async fn exec(
    runner: &ContainerAsync<GenericImage>,
    step: &'static str,
    text: String,
) -> Result<(), KeycloakError> {
    let docker = |source| KeycloakError::Docker { step, source };
    let command = ExecCommand::new(["bash".to_owned(), "-c".to_owned(), text])
        .with_cmd_ready_condition(CmdWaitFor::exit());
    let mut done = tokio::time::timeout(SCRIPT_BUDGET, runner.exec(command))
        .await
        .map_err(|_elapsed| {
            answer(
                step,
                StatusCode::GATEWAY_TIMEOUT,
                &"the script ran too long",
            )
        })?
        .map_err(docker)?;
    let mut output =
        String::from_utf8_lossy(&done.stdout_to_vec().await.map_err(docker)?).into_owned();
    output.push_str(&String::from_utf8_lossy(
        &done.stderr_to_vec().await.map_err(docker)?,
    ));
    let code = done.exit_code().await.map_err(docker)?;
    if code == Some(0) {
        return Ok(());
    }
    Err(KeycloakError::Script { step, code, output })
}

/// Requests a token at `endpoint` with `fields`.
async fn token(endpoint: &str, fields: &[(&str, &str)]) -> Result<String, KeycloakError> {
    let step = "the token request";
    let answered = reqwest::Client::new()
        .post(endpoint)
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(form(fields))
        .send()
        .await
        .map_err(|source| KeycloakError::Exchange { step, source })?;
    let status = answered.status();
    let body = answered
        .bytes()
        .await
        .map_err(|source| KeycloakError::Exchange { step, source })?;
    if status != StatusCode::OK {
        return Err(answer(step, status, &String::from_utf8_lossy(&body)));
    }
    serde_json::from_slice::<TokenAnswer>(&body)
        .map(|answer| answer.access_token)
        .map_err(|error| answer(step, status, &error))
}

/// `fields` as an `application/x-www-form-urlencoded` body.
fn form(fields: &[(&str, &str)]) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(fields)
        .finish()
}

/// The `Cookie` value that returns every cookie `headers` set.
fn cookies(headers: &http::HeaderMap) -> String {
    headers
        .get_all(SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(|value| value.split(';').next())
        .collect::<Vec<_>>()
        .join("; ")
}

/// The action of the login form on `page`, its `&amp;` entities decoded.
fn form_action(page: &str) -> Option<String> {
    let (_, form) = page.split_once("id=\"kc-form-login\"")?;
    let (_, rest) = form.split_once("action=\"")?;
    let (action, _) = rest.split_once('"')?;
    Some(action.replace("&amp;", "&"))
}

/// The [`KeycloakError::Answer`] of `step` answered `status`, missing
/// `detail`.
fn answer(step: &'static str, status: StatusCode, detail: &dyn std::fmt::Display) -> KeycloakError {
    KeycloakError::Answer {
        step,
        status,
        detail: detail.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::form_action;

    #[test]
    fn the_login_form_action_is_read_with_its_entities_decoded() {
        let page = r#"<a action="x"></a><form id="kc-form-login" class="f" action="http://kc/realms/r/login-actions/authenticate?session_code=a&amp;tab_id=b" method="post">"#;
        assert_eq!(
            Some(
                "http://kc/realms/r/login-actions/authenticate?session_code=a&tab_id=b".to_owned()
            ),
            form_action(page)
        );
        assert_eq!(None, form_action("<form action=\"x\">"));
    }
}
