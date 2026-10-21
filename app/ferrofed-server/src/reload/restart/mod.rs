// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a reload applies and what waits for a restart: the settings a
//! reload builds the federation from, and the changed keys it leaves to the
//! next start. Of `[federation]`, a reload applies the localization policy
//! (`[federation.localization]`) and the consent reporting
//! (`[federation.consent]`); of `[server]`, `[auth]` less its audience. No
//! specification governs this: our own design.

use crate::binding;
use crate::config::auth::{AuthSettings, Verification};
use crate::config::settings::{ServerSettings, Settings};

mod keys;

use keys::RESTART_KEYS;

/// Returns the core's keys a reload leaves to the next start.
pub(super) fn keys() -> Vec<&'static str> {
    RESTART_KEYS.iter().map(|(key, _)| *key).collect()
}

/// Returns `fresh`, its audiences the ones `boot` started with, at the
/// section and at every issuer that took the section's.
///
/// The audience also names the gateway in its access records, read once at
/// start, so a changed `[auth] audience` waits for a restart (no
/// specification governs this: our own design).
fn boot_audience(boot: &AuthSettings, mut fresh: AuthSettings) -> AuthSettings {
    for issuer in &mut fresh.issuers {
        let audience = match &mut issuer.verification {
            Verification::KeySet(settings) => &mut settings.audience,
            Verification::Introspection(endpoint) => &mut endpoint.audience,
        };
        if *audience == fresh.audience {
            audience.clone_from(&boot.audience);
        }
    }
    fresh.audience.clone_from(&boot.audience);
    fresh
}

/// The settings a reload builds the federation from: the sections of `fresh`
/// a reload applies, and every other setting as the process started, each
/// binding keeping the boot's value of what it does not reload.
pub(super) fn effective(boot: &Settings, fresh: Settings) -> Settings {
    let mut federation = boot.federation.clone();
    // NOTE: no specification governs this: our own design; the localization policy
    // applies only beside the node selection and budget it was checked with (§14.1).
    if fresh.federation.node_selection == boot.federation.node_selection
        && fresh.federation.budget == boot.federation.budget
    {
        federation.localization = fresh.federation.localization;
    }
    federation.consent_disclosure = fresh.federation.consent_disclosure;
    federation.consent_emergency = fresh.federation.consent_emergency;
    let auth = boot_audience(&boot.server.auth, fresh.server.auth.clone());
    let mut effective = Settings {
        profile: boot.profile,
        server: ServerSettings {
            auth,
            ..boot.server.clone()
        },
        telemetry: boot.telemetry.clone(),
        federation,
        stored_queries: boot.stored_queries.clone(),
        fhir: boot.fhir.clone(),
        metrics: boot.metrics.clone(),
        signing: boot.signing.clone(),
        audit: boot.audit.clone(),
        revisions: boot.revisions.clone(),
        revision_poll: boot.revision_poll,
        revision_policy: boot.revision_policy.clone(),
        usage_report: boot.usage_report.clone(),
        ..fresh
    };
    for binding in binding::compiled() {
        binding.effective(boot, &mut effective);
    }
    effective
}

/// The keys a reload does not apply whose value in `fresh` differs from the
/// one the process started with: the core's ([`RESTART_KEYS`]), then each
/// binding's.
pub(super) fn needs_restart(boot: &Settings, fresh: &Settings) -> Vec<&'static str> {
    RESTART_KEYS
        .iter()
        .filter_map(|(key, changed)| changed(boot, fresh).then_some(*key))
        .chain(
            binding::compiled()
                .iter()
                .flat_map(|binding| binding.needs_restart(boot, fresh)),
        )
        .collect()
}

#[cfg(test)]
#[cfg(feature = "binding-ihe")]
mod tests {
    use std::collections::BTreeMap;

    use super::{effective, needs_restart};
    use crate::audit::AuditDestination;
    use crate::config::Config;
    use crate::config::settings::Settings;

    /// The settings of a development gateway whose XCPD audit messages go
    /// to `audit`, with the `[xcpd.audit_repository]` keys `repository`.
    fn settings(audit: &str, repository: &str) -> Settings {
        let text = format!(
            "profile = \"development\"\n\n[registry]\ndocument = \"registry.toml\"\n\n[xcpd]\nsender_device = \"2.999.40.1\"\nhome_community = \"2.999.40\"\naudit = \"{audit}\"\n\n[[xcpd.gateway]]\nurl = \"https://xcpd.example.org/rg\"\ndevice = \"2.999.50.1\"\n{repository}"
        );
        Config::from_sources(Some(&text), &BTreeMap::new())
            .and_then(|config| config.resolve())
            .expect("the settings resolve")
    }

    #[test]
    fn where_the_audit_messages_go_takes_a_restart() {
        let boot = settings("log", "");
        let fresh = settings(
            "repository",
            "\n[xcpd.audit_repository]\nurl = \"tls://arr.example.org\"\nhostname = \"gateway.example.org\"\n",
        );
        assert_eq!(vec!["xcpd.audit"], needs_restart(&boot, &fresh));
        let applied = effective(&boot, fresh);
        let xcpd = applied.xcpd.expect("the xcpd section reloads");
        assert_eq!(
            AuditDestination::Log,
            xcpd.audit,
            "the boot's destination holds"
        );
        assert_eq!(None, xcpd.audit_repository);

        let same = settings("log", "");
        assert_eq!(needs_restart(&boot, &same), Vec::<&str>::new());
    }

    /// The settings of a development gateway asking the PDQm Supplier at
    /// `url` with `transaction`.
    fn with_pdqm(url: &str, transaction: &str) -> Settings {
        let text = format!(
            "profile = \"development\"\n\n[audit]\ndestination = \"log\"\n\n[pdqm]\nurl = \"{url}\"\ntransaction = \"{transaction}\"\nmaster = \"urn:oid:2.999.1\"\n\n[pdqm.namespaces]\n\"urn:oid:2.999.7\" = \"urn:oid:2.999.7\"\n"
        );
        Config::from_sources(Some(&text), &BTreeMap::new())
            .and_then(|config| config.resolve())
            .expect("the settings resolve")
    }

    #[test]
    fn a_change_to_the_demographics_step_reloads_with_the_resolver() {
        let boot = with_pdqm("https://pdq.example.org/fhir/", "iti-78");
        let fresh = with_pdqm("https://other.example.org/fhir/", "iti-119");
        assert!(
            needs_restart(&boot, &fresh).is_empty(),
            "[pdqm] is reloadable, as [pixm] is"
        );
        let applied = effective(&boot, fresh)
            .pdqm
            .expect("the pdqm section reloads");
        assert_eq!("https://other.example.org/fhir/", applied.url.expose());
    }
}
