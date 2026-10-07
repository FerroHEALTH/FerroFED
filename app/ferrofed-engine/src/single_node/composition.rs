// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A composition the gateway composes, committed to one node:
//! `POST {base}/v1/ehr/{ehr_id}/composition`, as openEHR ITS-REST 1.1.0
//! names it (`composition_create`).
//!
//! The call goes through the generated `EhrClient` of `openehr-its`'s
//! `rest-client`, with the endpoint's onward credentials, the call's deadline,
//! the caller's identity signed for the node and the gateway's minted
//! `X-Request-Id`, and it passes the outbound gate first: the node is located
//! by its own `ehr_id`, and the URL and every header the gateway sets are read
//! against the identifiers the options withhold (§5.4.1, N33). The
//! composition is a write body, which the gate never reads (§5.4 scope note).
//!
//! An error names the endpoint and the node's status, never the node's body,
//! which may echo the composition it was sent.

use openehr_its::rest::client::Transport;
use openehr_its::rest::generated::ehr::CompositionCreateParams;
use openehr_its::rest::generated::ehr::client::{CompositionCreateOutcome, EhrClient};
use openehr_rm::v1_2::composition::composition::Composition;

use crate::dispatch::{Contact, DispatchOptions, NodeClient};
use crate::single_node::ehr::{EhrCallError, from_etag, from_location};
use crate::trace_context;
use http::StatusCode;
use openehr_federation::status::EndpointStatus;

/// The `Prefer` value a create sends: the node answers with the new
/// version's uid in `ETag` and `Location` and no body (ITS-REST 1.1.0 EHR
/// API, `Prefer`).
const PREFER_MINIMAL: &str = "return=minimal";

impl<T: Transport + Clone> NodeClient<T> {
    /// Commits `composition` into the EHR `ehr_id` names on the node, and
    /// returns the `OBJECT_VERSION_ID` of the version the node created, as
    /// the node wrote it.
    ///
    /// The uid is read from `ETag`, in double quotes, or else from the last
    /// segment of `Location` (ITS-REST 1.1.0 EHR API, `201_COMPOSITION`).
    ///
    /// # Errors
    ///
    /// Returns [`EhrCallError::Withheld`] with nothing sent,
    /// [`EhrCallError::Expired`] when the deadline passed before it left,
    /// [`EhrCallError::TimeOut`] and [`EhrCallError::Unreachable`] when the
    /// node gave no answer, [`EhrCallError::Rejected`] for a `400`, `404` or
    /// `422`, [`EhrCallError::Unversioned`] for a success naming no version,
    /// and [`EhrCallError::Failed`] for every other failure.
    pub async fn create_composition(
        &self,
        ehr_id: &str,
        composition: &Composition,
        options: &DispatchOptions,
    ) -> Result<String, EhrCallError> {
        trace_context::node_request(
            self.endpoint(),
            "composition_create",
            self.create_composition_once(ehr_id, composition, options),
            |created| match created {
                Ok(_) => (None, Some(EndpointStatus::Active)),
                Err(error) => (Some(Contact::of_ehr_call_error(error)), None),
            },
        )
        .await
    }

    /// Commits the composition once, as [`NodeClient::create_composition`]
    /// describes.
    async fn create_composition_once(
        &self,
        ehr_id: &str,
        composition: &Composition,
        options: &DispatchOptions,
    ) -> Result<String, EhrCallError> {
        let params = CompositionCreateParams {
            ehr_id: ehr_id.to_owned(),
            prefer: Some(PREFER_MINIMAL.to_owned()),
            accept: None,
            content_type: None,
            openehr_item_tag: None,
            openehr_version_item_tag: None,
            openehr_version: None,
            openehr_audit_details: None,
            openehr_template_id: None,
        };
        let path = format!(
            "/ehr/{}/composition",
            openehr_its::rest::client::path_segment(&params.ehr_id)
        );
        self.gate_ehr(
            (&path, Some(ehr_id)),
            &[("Prefer", PREFER_MINIMAL)],
            options,
        )?;
        let call = options
            .call_options(self.endpoint())
            .map_err(|error| self.options_failure(error))?;
        let _slot = self.slot(options.deadline()).await?;
        let client = self.client_for(options);
        let answer = EhrClient::new(&client)
            .with_options(call)
            .composition_create(&params, composition)
            .await
            .map_err(|source| self.ehr_failure(source))?;
        let (status, etag, location) = match answer {
            CompositionCreateOutcome::Created { headers, .. } => {
                (StatusCode::CREATED, headers.etag, headers.location)
            }
            CompositionCreateOutcome::NoContent { headers } => {
                (StatusCode::NO_CONTENT, headers.etag, headers.location)
            }
            CompositionCreateOutcome::BadRequest { body } => {
                return Err(self.rejected(StatusCode::BAD_REQUEST, body));
            }
            CompositionCreateOutcome::NotFound { body } => {
                return Err(self.rejected(StatusCode::NOT_FOUND, body));
            }
            CompositionCreateOutcome::UnprocessableEntity { body } => {
                return Err(self.rejected(StatusCode::UNPROCESSABLE_ENTITY, body));
            }
        };
        etag.as_deref()
            .and_then(from_etag)
            .or_else(|| location.as_deref().and_then(from_location))
            .ok_or_else(|| EhrCallError::Unversioned {
                endpoint: self.endpoint().clone(),
                status,
            })
    }
}
