// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A `serve` process over `[server.tls]`: it presents the certificate its
//! files hold, `ferrofed healthcheck` follows it over TLS, and a `SIGHUP`
//! after the files are replaced presents the new certificate with no
//! restart, while the healthcheck reports the replaced file until then.

use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use ferrofed_server::healthcheck::READINESS;
use ferrofed_testkit::listener::ListenerCertificates;
use http::StatusCode;

use super::{TestResult, client, ferrofed, presented, quoted, written};
use crate::support::auth_toml;

/// How long the process may take to listen, and to take the new files.
const PATIENCE: Duration = Duration::from_secs(20);

/// A `serve` process killed when the test ends, however it ends.
struct Serving(Child);

impl Drop for Serving {
    fn drop(&mut self) {
        // The process may already have exited; nothing is left to stop.
        let _killed: std::io::Result<()> = self.0.kill();
        let _reaped: std::io::Result<ExitStatus> = self.0.wait();
    }
}

impl Serving {
    /// Sends `signal`, such as `HUP`, to the process.
    fn signal(&self, signal: &str) -> TestResult {
        let sent = Command::new("kill")
            .args([&format!("-{signal}"), &self.0.id().to_string()])
            .status()?;
        assert!(sent.success(), "SIG{signal} was sent");
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn sighup_presents_the_replaced_certificate_without_a_restart() -> TestResult {
    let dir = tempfile::tempdir()?;
    let first = ListenerCertificates::generate()?;
    let files = first.write(dir.path())?;
    let listen = std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?;
    let config = dir.path().join("ferrofed.toml");
    std::fs::write(
        &config,
        format!(
            "[server]\nlisten = \"{listen}\"\n\n[server.tls]\ncertificate_file = {}\nkey_file = {}\n{}",
            quoted(&files.certificate),
            quoted(&files.key),
            auth_toml()?,
        ),
    )?;
    let serving = Serving(
        Command::new(env!("CARGO_BIN_EXE_ferrofed"))
            .args(["serve", "--config"])
            .arg(&config)
            .env_remove("FERROFED_CONFIG")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let trusting_first = client(first.ca(), None)?;
    let started = Instant::now();
    let leaf = loop {
        if let Ok((StatusCode::OK, leaf)) = presented(&trusting_first, listen, READINESS).await {
            break leaf;
        }
        if started.elapsed() > PATIENCE {
            return Err("the gateway never answered over TLS".into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(first.leaf(), leaf.as_slice());

    let healthcheck = || {
        let config = config.clone();
        tokio::task::spawn_blocking(move || ferrofed(&["healthcheck"], &config))
    };
    let output = healthcheck().await??;
    assert_eq!(Some(0), output.status.code(), "{}", written(&output));

    let second = ListenerCertificates::generate()?;
    second.write(dir.path())?;
    let output = healthcheck().await??;
    assert_eq!(Some(1), output.status.code(), "{}", written(&output));
    assert!(
        written(&output).contains("SIGHUP"),
        "the replaced file is named as the cause: {}",
        written(&output)
    );

    serving.signal("HUP")?;
    let trusting_second = client(second.ca(), None)?;
    let started = Instant::now();
    loop {
        if let Ok((StatusCode::OK, leaf)) = presented(&trusting_second, listen, READINESS).await
            && leaf.as_slice() == second.leaf()
        {
            break;
        }
        if started.elapsed() > PATIENCE {
            return Err("the gateway never presented the replaced certificate".into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let output = healthcheck().await??;
    assert_eq!(Some(0), output.status.code(), "{}", written(&output));
    serving.signal("TERM")?;
    Ok(())
}
