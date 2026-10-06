// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Integration tests through the library run path the binary shares.

#[cfg(feature = "binding-ihe")]
mod access;
mod admin_peer;
mod admin_record;
mod admission;
mod aggregate;
mod ask_all;
#[cfg(feature = "binding-ihe")]
mod audit_repository;
mod auth;
mod banner;
mod base_url;
mod caller_bindings;
mod client_address;
mod completeness;
mod config;
mod conformance_run;
mod consent;
mod consent_everywhere;
mod consent_withheld;
mod contact_point;
mod contribution_write;
mod conveyance;
mod created_ehr_id;
mod credentials;
mod declared;
mod dedup;
mod dedup_write;
mod definition;
mod demographic;
mod deployment_decisions;
mod directive;
mod distinct;
mod e2e;
mod ehr_by_subject;
mod ehr_id_collision;
mod ehr_scope;
mod endpoint_attributes;
mod endpoint_report;
mod errors;
mod facade;
#[cfg(feature = "binding-ihe")]
mod feed_audit;
mod fhir;
mod follow_up;
mod healthcheck;
mod http;
mod hygiene;
#[cfg(feature = "binding-ihe")]
mod identity_grant;
mod its_rest_areas;
mod lifecycle;
mod listener_tls;
mod localization;
#[cfg(feature = "binding-ihe")]
mod localizer_audit;
mod localizer_surface;
mod manufacturer;
mod metrics;
#[cfg(feature = "binding-nl")]
mod mitz;
#[cfg(feature = "binding-ihe")]
mod mutual_tls;
mod next_key;
#[cfg(feature = "binding-nl")]
mod nl_gf;
mod no_destination;
mod node_profile;
#[cfg(feature = "binding-nl")]
mod nvi_nuts;
mod onward;
mod onward_exchange;
mod onward_fapi2;
mod onward_mtls;
#[cfg(feature = "binding-nl")]
mod onward_nuts;
mod operator;
mod options;
mod order;
mod order_key;
mod outbound;
mod outbound_id;
mod overload;
mod path_ehr_id;
#[cfg(feature = "binding-ihe")]
mod pdqm;
#[cfg(feature = "binding-ihe")]
mod pixm_localizer;
#[cfg(feature = "binding-ihe")]
mod pmir;
mod probed_ehr_id;
mod provenance;
mod public_url;
mod query_get;
mod query_media;
mod readiness;
mod registry_fhir;
#[cfg(feature = "binding-ihe")]
mod registry_mcsd;
mod reload;
mod report;
mod request_log;
#[cfg(feature = "binding-ihe")]
mod resolution;
mod route_log;
mod routing;
mod routing_log;
mod run;
mod seed_data;
mod shutdown;
mod stored;
mod stored_fan_out;
mod stored_redistribution;
mod support;
mod targeting;
mod telemetry;
mod template_fan_out;
mod timeouts;
mod traces;
mod track10;
mod transport;
mod troubleshooting;
mod versioned_write;
#[cfg(feature = "binding-ihe")]
mod xcpd;
