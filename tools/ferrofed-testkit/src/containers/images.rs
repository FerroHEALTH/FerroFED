// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The images the harness starts, each pinned by tag and digest.
//!
//! Every image is a [`PinnedImage`] constant here, which `docs/VERSIONS.md`
//! repeats and `scripts/checks/versions.sh` compares, and which
//! `scripts/checks/pin-freshness.sh` reads against its registry.

use testcontainers::GenericImage;

/// One container image, pinned by tag and by digest.
///
/// The digest is what Docker resolves; the tag travels beside it so a reader
/// sees which release the digest is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinnedImage {
    /// The repository, registry included for anything but Docker Hub.
    pub repository: &'static str,
    /// The tag the digest was published under.
    pub tag: &'static str,
    /// The `sha256:` digest of the image index.
    pub digest: &'static str,
}

impl PinnedImage {
    /// Returns the reference Docker resolves this image by.
    ///
    /// # Examples
    ///
    /// ```
    /// let reference = ferrofed_testkit::containers::images::FERROEHR.reference();
    /// assert!(reference.starts_with("ghcr.io/ferrohealth/ferroehr:"));
    /// assert!(reference.contains("@sha256:"));
    /// ```
    #[must_use]
    pub fn reference(&self) -> String {
        format!("{}:{}@{}", self.repository, self.tag, self.digest)
    }

    /// Returns the image with the digest in the tag position, which is how
    /// `testcontainers` spells a reference (`name:tag`).
    pub(crate) fn image(&self) -> GenericImage {
        GenericImage::new(
            self.repository.to_owned(),
            format!("{}@{}", self.tag, self.digest),
        )
    }
}

/// FerroEHR, an openEHR CDR speaking ITS-REST 1.1.0, which both nodes run.
pub const FERROEHR: PinnedImage = PinnedImage {
    repository: "ghcr.io/ferrohealth/ferroehr",
    tag: "5.4.1",
    digest: "sha256:b676e89db9cdd808817f7cfd46e13e79415e63c3baf92b5f249da38bd2a5e732",
};

/// The database image FerroEHR documents, which carries the role, the
/// database and the extensions its migrations expect.
pub const FERROEHR_POSTGRES: PinnedImage = PinnedImage {
    repository: "ghcr.io/ferrohealth/ferroehr-postgres",
    tag: "5.4.1",
    digest: "sha256:eb3a720d6245f5472fe5f70ab66ed9662fe6addfa9c543f470ebc689f2eb2cfe",
};

/// EHRbase, an openEHR CDR of another vendor speaking ITS-REST, which the
/// node profile runs against beside FerroEHR.
pub const EHRBASE: PinnedImage = PinnedImage {
    repository: "ehrbase/ehrbase",
    tag: "2.36.0",
    digest: "sha256:c8e642264b73637e0576ec01b5c73f5dc9be6f34eb3644f0ced890c5f916640a",
};

/// The database image EHRbase documents beside that release, which creates
/// its database, its two login roles and its schemas.
pub const EHRBASE_POSTGRES: PinnedImage = PinnedImage {
    repository: "ehrbase/ehrbase-v2-postgres",
    tag: "16.2",
    digest: "sha256:abe14e8f9ba33cabc9946c6c17c5aa95b64b35387f266cd20a894149203196d7",
};

/// FerroPIX, the FerroHEALTH master patient index, whose PIXm Manager
/// answers ITI-83 and takes the ITI-104 feed: the PIX Manager the end-to-end
/// lane resolves patients through.
pub const FERROPIX: PinnedImage = PinnedImage {
    repository: "ghcr.io/ferrohealth/ferropix",
    tag: "0.0.4",
    digest: "sha256:7568cdb740fe3938d99ad51bdda47c464e10042a103e6d30b0f64d03496fbc73",
};

/// The PostgreSQL FerroPIX keeps its cross-reference in: the release FerroPIX
/// 0.0.4 pins for its own tests and quickstart, PostgreSQL 18.
pub const FERROPIX_POSTGRES: PinnedImage = PinnedImage {
    repository: "postgres",
    tag: "18.6",
    digest: "sha256:edb0c08dc33befd0262ab3b5cf1c4f3f8cfd8b09e88c05b45adee29d6761ddb6",
};

/// SanteMPI, SanteSuite's open-source master patient index, which answers
/// PIXm ITI-83 and takes the PMIR ITI-93 feed: the deployable PIX Manager
/// the identity binding is verified against.
pub const SANTEMPI: PinnedImage = PinnedImage {
    repository: "santesuite/santedb-mpi",
    tag: "2.5.12",
    digest: "sha256:608484de046a932ec2f92e9991a32507fc8ec89d53d7639cbab886a63dbf6207",
};

/// The PostgreSQL SanteMPI runs on: SanteSuite's compose file names the
/// official image, and this is the release line current when [`SANTEMPI`]
/// was published.
pub const SANTEMPI_POSTGRES: PinnedImage = PinnedImage {
    repository: "postgres",
    tag: "15.19",
    digest: "sha256:724292da1f2e50bdccfc3302ce75bbba7f4a6076701b588cc795fcac65683550",
};

/// The Maven image the Federation Tier reference implementation is built
/// in, on the Java release its build declares (`java.version` 21).
pub const MAVEN: PinnedImage = PinnedImage {
    repository: "maven",
    tag: "3.10.0-eclipse-temurin-21",
    digest: "sha256:9b4877723dadf350b452dd97d9a6401e7b56f98fa9dfe420c32ad909989c7e4c",
};

/// The Java runtime image the reference implementation runs on, and the
/// Keycloak recipe's admin CLI.
pub const TEMURIN_JRE: PinnedImage = PinnedImage {
    repository: "eclipse-temurin",
    tag: "21.0.12.1_1-jre-noble",
    digest: "sha256:000fd431958bc81a24abe1e8e5f0f0fd3ae365a594bd50aadb20696805f9408c",
};

/// Keycloak, the identity provider the production guide's issuer recipe is
/// written for and [`keycloak`](mod@super::keycloak) applies.
pub const KEYCLOAK: PinnedImage = PinnedImage {
    repository: "quay.io/keycloak/keycloak",
    tag: "26.8.1",
    digest: "sha256:a78d6b79572168cb25886b80a8b5b8d4ed4390743e3eecb810538683e209dd6b",
};
