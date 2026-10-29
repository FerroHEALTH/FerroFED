// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `config check`: the report on a configuration that loads.

use std::process::ExitCode;

use crate::config::fhir::FhirSettings;
use crate::config::server::ListenerTlsSettings;
use crate::config::settings::Settings;
use crate::support::Support;
use crate::{admin, binding, body, config, metrics};

/// Reports the resolved configuration `settings`, its `cleartext`
/// credentials, what a remote admin listener serves, every deprecated key
/// it sets, every issuer that declares no assurance, an access log that
/// declares no emergency purpose and a release past its support period, and
/// exits.
#[expect(
    clippy::print_stdout,
    reason = "`config check` answers the person or pipeline that ran it"
)]
pub(super) fn config_checked(
    cleartext: &[config::transport::ProtectedSite],
    settings: &Settings,
) -> ExitCode {
    config::transport::print_warnings(cleartext);
    // NOTE: Regulation (EU) 2024/2847 Art 13(19): a release past the end of its
    // support period says so to the operator who checks its configuration.
    if let Some(notice) = Support::current().notice(body::VERSION) {
        println!("ferrofed: warning: {notice}");
    }
    for key in &settings.deprecated {
        println!(
            "ferrofed: warning: {} is deprecated and read as {}; rename it, because FerroFED {} refuses it",
            key.old, key.new, key.refused_in
        );
    }
    for note in settings
        .server
        .auth
        .notes()
        .into_iter()
        .chain(settings.access_log.notes())
        .chain(settings.fhir.iter().flat_map(FhirSettings::notes))
        .chain(
            binding::compiled()
                .iter()
                .flat_map(|binding| binding.notes(settings)),
        )
    {
        println!("ferrofed: note: {note}");
    }
    let surface = &settings.metrics;
    if let Some(address) = surface.listen.filter(|address| !address.ip().is_loopback()) {
        let scrape = if !surface.scrape_users.is_empty() {
            "a scrape that authenticates as a metrics.scrape_users user with HTTP Basic"
        } else if surface
            .tls
            .as_ref()
            .is_some_and(ListenerTlsSettings::requires_client_certificate)
        {
            "a client whose certificate metrics.tls.client_ca_file admits (client_auth = \"required\")"
        } else {
            "every peer"
        };
        println!(
            "ferrofed: note: metrics.listen {address} is not a loopback address: GET {} answers {scrape}, and the admin write actions ({}, {}, {}) answer a caller whose token carries [auth] product_scope",
            metrics::PATH,
            admin::DISTRIBUTE,
            admin::CLEAR_EHR_ID,
            admin::EXPORT_IDENTITY_STATE
        );
    }
    println!("ferrofed: the configuration is valid");
    ExitCode::SUCCESS
}
