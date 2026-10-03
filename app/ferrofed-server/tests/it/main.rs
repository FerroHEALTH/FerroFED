// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Integration tests through the library run path the binary shares.

mod aggregate;
mod ask_all;
mod completeness;
mod config;
mod credentials;
mod dedup;
mod directive;
mod distinct;
mod e2e;
mod e2e_attributes;
mod endpoint_attributes;
mod endpoint_report;
mod errors;
mod facade;
mod http;
mod hygiene;
mod no_destination;
mod order;
mod outbound;
mod outbound_id;
mod path_ehr_id;
mod readiness;
mod registry_fhir;
mod request_log;
mod resolution;
mod routing;
mod routing_log;
mod run;
mod shutdown;
mod support;
mod targeting;
mod telemetry;
mod timeouts;
mod track10;
