// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console of the Kubernetes package, deploy/kubernetes/components/console
//! (#638): the configuration its `ConfigMap` carries is the one `config check`
//! accepts, with a synthetic file for its client secret, and it names the
//! paths and the gateway the Deployment and the base give it. No
//! specification governs the packaging: our own design.

use std::collections::BTreeMap;
use std::error::Error;

use ferrofed_console::config::Config;
use ferrofed_console::server::ConsoleState;

/// The console component's `ConfigMap`.
const CONFIGMAP: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../deploy/kubernetes/components/console/configmap.yaml"
));

/// The console component's `Deployment`.
const DEPLOYMENT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../deploy/kubernetes/components/console/deployment.yaml"
));

/// Where the Deployment mounts the console's Secret.
const SECRETS: &str = "/run/secrets/ferrofed-console/";

/// The block scalar `key` holds under `data:`, its four-space indent removed.
fn data_key(yaml: &str, key: &str) -> String {
    let header = format!("  {key}: |");
    let mut lines = yaml.lines().skip_while(|line| *line != header).skip(1);
    let mut out = String::new();
    for line in lines.by_ref() {
        if let Some(rest) = line.strip_prefix("    ") {
            out.push_str(rest);
        } else if !line.trim().is_empty() {
            break;
        }
        out.push('\n');
    }
    out
}

#[test]
fn the_packaged_console_configuration_passes_config_check() -> Result<(), Box<dyn Error>> {
    let text = data_key(CONFIGMAP, "console.toml");
    assert!(
        text.contains("[oidc]"),
        "the ConfigMap carries console.toml"
    );
    assert!(
        DEPLOYMENT.contains("value: /etc/ferrofed-console/console.toml")
            && DEPLOYMENT.contains("mountPath: /etc/ferrofed-console")
            && DEPLOYMENT.contains(&format!("mountPath: {}", SECRETS.trim_end_matches('/'))),
        "the Deployment mounts the configuration and the Secret where it names them"
    );
    let dir = tempfile::tempdir()?;
    std::fs::write(
        dir.path().join("client-secret"),
        "synthetic-client-secret\n",
    )?;
    let local = text.replace(SECRETS, &format!("{}/", dir.path().display()));
    let settings = Config::from_sources(Some(&local), &BTreeMap::new())?.resolve()?;
    assert_eq!(
        "http://ferrofed:8080/",
        settings.gateway.base.as_str(),
        "the gateway's Service"
    );
    assert!(settings.oidc.is_some(), "the console offers sign-in");
    ConsoleState::new(settings)?;
    Ok(())
}
