// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the running binary knows of its own build, and the identifiers of
//! the release artefacts and attestations it was published with.
//!
//! The release lane sets `FERROFED_BUILD_COMMIT` and
//! `FERROFED_BUILD_TARGET` when it compiles the binary, so a released
//! binary names its commit and its target; a local build names neither and
//! says so. The image digest is not known inside the process: the report
//! documentation says how to read it from the container runtime. No
//! specification governs this: our own design.

use serde::Serialize;

use crate::banner::PINS;
use crate::body::{PRODUCT, VERSION};

/// The commit the release lane built from, when it set one.
const COMMIT: Option<&str> = option_env!("FERROFED_BUILD_COMMIT");

/// The target triple the release lane built for, when it set one.
const TARGET: Option<&str> = option_env!("FERROFED_BUILD_TARGET");

/// The repository every release is published from.
const REPOSITORY: &str = "FerroHEALTH/FerroFED";

/// The gateway's container image.
const IMAGE: &str = "ghcr.io/ferrohealth/ferrofed";

/// The workflow that signs each release tarball's provenance and SBOMs.
const TARBALL_SIGNER: &str = "FerroHEALTH/FerroFED/.github/workflows/release-build.yml";

/// The workflow that signs the image's provenance and SBOMs.
const IMAGE_SIGNER: &str = "FerroHEALTH/FerroFED/.github/workflows/release-image.yml";

/// The build of the running binary: `build.json` in the bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Build {
    /// The product name.
    pub product: &'static str,
    /// The product version.
    pub version: &'static str,
    /// The commit the binary was built from, when the build recorded it.
    pub commit: Option<&'static str>,
    /// The target triple, when the build recorded it.
    pub target: Option<&'static str>,
    /// The architecture, the operating system and the C library the binary
    /// was compiled for.
    pub platform: Platform,
    /// Whether the binary was built with optimizations and without debug
    /// assertions, as a release is.
    pub release_profile: bool,
    /// The Cargo features compiled in: the bindings and the optional stores.
    pub features: Vec<&'static str>,
    /// The specification releases and the `openehr-*` family the gateway
    /// is pinned to, as the startup banner prints them.
    pub pins: Vec<Pin>,
}

/// The platform a binary was compiled for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Platform {
    /// `std::env::consts::ARCH`.
    pub arch: &'static str,
    /// `std::env::consts::OS`.
    pub os: &'static str,
    /// The C library: `gnu`, `musl`, or empty.
    pub env: &'static str,
}

/// One pinned release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Pin {
    /// What is pinned.
    pub name: &'static str,
    /// The release.
    pub release: &'static str,
}

impl Build {
    /// The build of this binary.
    #[must_use]
    pub fn current() -> Self {
        let features = [
            ("binding-ihe", cfg!(feature = "binding-ihe")),
            ("binding-nl", cfg!(feature = "binding-nl")),
            ("postgres", cfg!(feature = "postgres")),
        ]
        .into_iter()
        .filter_map(|(name, on)| on.then_some(name))
        .collect();
        Self {
            product: PRODUCT,
            version: VERSION,
            commit: COMMIT.filter(|commit| !commit.is_empty()),
            target: TARGET.filter(|target| !target.is_empty()),
            platform: Platform {
                arch: std::env::consts::ARCH,
                os: std::env::consts::OS,
                env: if cfg!(target_env = "musl") {
                    "musl"
                } else if cfg!(target_env = "gnu") {
                    "gnu"
                } else {
                    ""
                },
            },
            release_profile: !cfg!(debug_assertions),
            features,
            pins: PINS
                .iter()
                .map(|&(name, release)| Pin { name, release })
                .collect(),
        }
    }
}

/// The release artefacts of this version and how each is verified:
/// `release.json` in the bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Release {
    /// The repository the release is published from.
    pub repository: &'static str,
    /// The release tag.
    pub tag: String,
    /// The release tarball of this binary's target, when the build recorded
    /// its target.
    pub tarball: Option<String>,
    /// The container image of this version.
    pub image: String,
    /// The workflow that signed the tarball's attestations.
    pub tarball_signer: &'static str,
    /// The workflow that signed the image's attestations.
    pub image_signer: &'static str,
    /// The commands that verify the attestations of the tarball and the
    /// image.
    pub verify: Vec<String>,
}

impl Release {
    /// The release of `build`.
    #[must_use]
    pub fn of(build: &Build) -> Self {
        let tag = format!("v{}", build.version);
        let tarball = build
            .target
            .map(|target| format!("ferrofed-{tag}-{target}.tar.gz"));
        let image = format!("{IMAGE}:{}", build.version);
        let mut verify = Vec::new();
        if let Some(tarball) = &tarball {
            verify.push(format!(
                "gh attestation verify {tarball} --repo {REPOSITORY} --signer-workflow {TARBALL_SIGNER}"
            ));
        }
        verify.push(format!(
            "gh attestation verify oci://{image} --repo {REPOSITORY} --signer-workflow {IMAGE_SIGNER}"
        ));
        Self {
            repository: REPOSITORY,
            tag,
            tarball,
            image,
            tarball_signer: TARBALL_SIGNER,
            image_signer: IMAGE_SIGNER,
            verify,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Build, Release};

    #[test]
    fn the_build_names_the_version_the_features_and_every_banner_pin() {
        let build = Build::current();
        assert_eq!(env!("CARGO_PKG_VERSION"), build.version);
        assert_eq!(4, build.pins.len());
        assert_eq!(
            cfg!(feature = "binding-ihe"),
            build.features.contains(&"binding-ihe")
        );
    }

    #[test]
    fn a_release_names_its_tarball_only_for_a_recorded_target() {
        let mut build = Build::current();
        build.version = "1.2.3";
        build.target = Some("x86_64-unknown-linux-musl");
        let release = Release::of(&build);
        assert_eq!("v1.2.3", release.tag);
        assert_eq!(
            Some("ferrofed-v1.2.3-x86_64-unknown-linux-musl.tar.gz"),
            release.tarball.as_deref()
        );
        assert_eq!("ghcr.io/ferrohealth/ferrofed:1.2.3", release.image);
        assert_eq!(2, release.verify.len());
        build.target = None;
        let release = Release::of(&build);
        assert_eq!(None, release.tarball);
        assert_eq!(1, release.verify.len(), "the image alone");
    }
}
