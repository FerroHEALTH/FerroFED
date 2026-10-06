// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The manufacturer named in the running system, on each surface the
//! gateway shows it on: the startup banner, `ferrofed --version` and the
//! container image labels (Regulation (EU) 2025/327 Art 30(1)(g)). The
//! root document and `OPTIONS {base}/` are held in `http.rs` and
//! `options.rs`.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::process::Command;

use clap::CommandFactory as _;
use ferrofed_registry::manufacturer::MANUFACTURER;
use ferrofed_server::banner::{Deployment, Registry, render};
use ferrofed_server::cli::Cli;
use ferrofed_server::support::Support;

type TestResult = Result<(), Box<dyn Error>>;

/// The name, the postal address and the single point of contact, as each
/// surface must show them.
const PINNED: [&str; 3] = [
    "Cadasto B.V.",
    "Comeniusstraat 2d, 1817 MS Alkmaar, The Netherlands",
    "info@cadasto.com",
];

#[test]
fn the_pinned_text_is_the_manufacturer() {
    assert_eq!(
        [
            MANUFACTURER.name,
            MANUFACTURER.postal_address,
            MANUFACTURER.email
        ],
        PINNED
    );
}

#[test]
fn the_banner_names_the_manufacturer_its_contact_and_its_address() -> TestResult {
    let deployment = Deployment {
        base_path: "/federation".parse()?,
        listen: "127.0.0.1:8080".parse()?,
        registry: Registry::Unset,
        stored_queries: None,
        development: false,
        cleartext: Vec::new(),
        audit_spool_in_memory: false,
        support: Support::NoPeriod,
    };
    let banner = render("9.9.9", &deployment, false);
    assert!(
        banner.contains("Manufactured by Cadasto B.V. · info@cadasto.com"),
        "{banner}"
    );
    for text in PINNED {
        assert!(banner.contains(text), "{text}: {banner}");
    }
    Ok(())
}

#[test]
fn the_long_version_names_the_manufacturer() {
    let version = Cli::command().render_long_version();
    assert!(
        version.starts_with(&format!("ferrofed {}\n", env!("CARGO_PKG_VERSION"))),
        "{version}"
    );
    for text in PINNED {
        assert!(version.contains(text), "{text}: {version}");
    }
    assert!(version.contains(MANUFACTURER.website), "{version}");
}

#[test]
fn the_binary_prints_the_manufacturer_on_version() -> TestResult {
    let output = Command::new(env!("CARGO_BIN_EXE_ferrofed"))
        .arg("--version")
        .output()?;
    assert!(output.status.success(), "{output:?}");
    let printed = String::from_utf8(output.stdout)?;
    for text in PINNED {
        assert!(printed.contains(text), "{text}: {printed}");
    }
    Ok(())
}

#[test]
fn every_image_label_set_names_the_manufacturer() -> TestResult {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let line = MANUFACTURER.line();
    let quoted = [
        format!(r#"org.opencontainers.image.vendor="{}""#, MANUFACTURER.name),
        format!(
            r#"org.opencontainers.image.authors="{} <{}>""#,
            MANUFACTURER.name, MANUFACTURER.email
        ),
        format!(r#"eu.ferrofed.manufacturer="{line}""#),
    ];
    for dockerfile in ["docker/Dockerfile", "docker/viewer/Dockerfile"] {
        let text = std::fs::read_to_string(format!("{root}/{dockerfile}"))?;
        for label in &quoted {
            assert!(text.contains(label.as_str()), "{dockerfile}: {label}");
        }
    }
    let bare = [
        format!("org.opencontainers.image.vendor={}\n", MANUFACTURER.name),
        format!(
            "org.opencontainers.image.authors={} <{}>\n",
            MANUFACTURER.name, MANUFACTURER.email
        ),
        format!("eu.ferrofed.manufacturer={line}\n"),
    ];
    for workflow in [
        ".github/workflows/release-image.yml",
        ".github/workflows/release-viewer.yml",
    ] {
        let text = std::fs::read_to_string(format!("{root}/{workflow}"))?;
        for label in &bare {
            assert_eq!(
                2,
                text.matches(label.as_str()).count(),
                "{workflow}, in its labels and its annotations: {label}"
            );
        }
    }
    Ok(())
}
