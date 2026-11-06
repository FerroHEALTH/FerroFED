// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The fixed message of each [`Code`], for a failure that has no more to
//! say than its code.

use super::Code;

impl Code {
    /// The fixed `message` of a failure that has no more to say than its
    /// code.
    #[must_use]
    #[expect(
        clippy::too_many_lines,
        reason = "one arm per code: the table of every fixed message"
    )]
    pub fn message(self) -> &'static str {
        match self {
            Self::BodyInvalid => "the request body is not the ITS-REST request this route takes",
            Self::CompletenessInvalid => {
                "the openEHR-federation-completeness header takes \"all\" or \"partial\", once"
            }
            Self::PartialUnsupported => "best-effort completion is not offered (§11.4)",
            Self::DedupInvalid => {
                "the openEHR-federation-dedup header takes \"none\" or \"version-identity\", once"
            }
            Self::ParameterInvalid => "a query parameter is not an AQL literal",
            Self::PatientInvalid => "the patient reference cannot be formed (§5.2)",
            Self::Refused(_) => "the query is refused",
            Self::NoDestination => "the request can be routed to no destination (§11.2)",
            Self::EhrIdCollision => "the ehr_id is claimed by more than one node (N42)",
            Self::ControllingSystemUnreachable => {
                "no reachable node is the controlling system of this version (N36)"
            }
            Self::Internal => "the gateway failed on its own side",
            Self::NotFound => "no resource is served at this path",
            Self::NotImplemented => "this part of the ITS-REST API is not served by the gateway",
            Self::EndpointUnknown => {
                "the endpoint directive or header names an endpoint the registry does not know (§8.4.1)"
            }
            Self::OrganisationUnknown => {
                "the organisation directive or header names an organisation the registry does not know (§8.4.1)"
            }
            Self::TargetRequired => {
                "a write that no binding or index routes to one node, the creation of an EHR and a definition request name their node in the openEHR-federation-endpoint header (§12.4, §12.5.1, §12.6, N23, N41, N43)"
            }
            Self::EndpointSeveral => {
                "a request routed to one node selects exactly one endpoint through its targeting headers (§7a.1)"
            }
            Self::QueryParameterRefused => {
                "a query parameter the ITS-REST operation does not declare is refused, never forwarded (§5.4.1, N33)"
            }
            Self::NodeTimeout => "the node did not answer in time (§11.2)",
            Self::NodeUnreachable => "the node could not be reached (§11.2)",
            Self::NodeRefused => "the node refused the gateway's onward credentials (§11.2)",
            Self::TargetingConflict => {
                "the request's targeting mechanisms select different node sets (§8.4.1, N35)"
            }
            Self::EhrIdInvalid => "the ehr_id in the path is not an openEHR HIER_OBJECT_ID (§12.5)",
            Self::NodeError => "a node answered with an error (§11.2)",
            Self::ProbeRequiresUuid => {
                "a read of an EHR resource that no header, binding or index routes to one node is probed at every member only when its ehr_id is a UUID, so name its node in the openEHR-federation-endpoint header (§5.4.1, N33, §12.5.1)"
            }
            Self::QueryNameInvalid => {
                "a stored query name is [{namespace}::]{query-name} over a-z, A-Z, 0-9, _, . and -, and never aql"
            }
            Self::QueryVersionInvalid => {
                "a stored query version is major.minor.patch, or {major} or {major}.{minor} where a version is looked up"
            }
            Self::QueryTypeUnsupported => "the registry stores AQL only",
            Self::SubjectLiteral => {
                "a stored query names its patient through a $parameter, never a literal the registry would hold (§5.4.1, N33)"
            }
            Self::StoredQueryHeld => {
                "the registry holds this name and version, and a stored version is immutable: store a new version (§12.7, N44)"
            }
            Self::StoredQueryUnknown => {
                "the registry holds no stored query at this name and version"
            }
            Self::PrecedingVersionInvalid => {
                "a versioned write names the version it amends as one quoted OBJECT_VERSION_ID in If-Match, or in the path of a DELETE, so its controlling CDR can be found (§12.4, N23)"
            }
            Self::ParameterValueInvalid => {
                "a path identifier, query value or header routed to one node matches what its ITS-REST operation declares, or nothing is sent (§5.4.1, N33)"
            }
            Self::MediaTypeNotAcceptable => {
                "the Accept header admits none of the media types the ITS-REST operation answers in"
            }
            Self::MediaTypeUnsupported => {
                "the Content-Type header is not a media type the ITS-REST operation takes"
            }
            Self::SubjectSeveral => {
                "the subject resolves at more than one member, and the gateway never chooses between them: name one in the openEHR-federation-endpoint header (§8.4, §12.5.2, N41)"
            }
            Self::ResolutionUnavailable => {
                "the cross-reference service could not answer, so where the subject has an EHR is unknown (§5.2, §11.2)"
            }
            Self::EhrIdHeld => {
                "the ehr_id is already held at another member, so no EHR is created under it at the endpoint named (§12.4, §12.5.2)"
            }
            Self::DefinitionEndpointTargeted => {
                "a stored query whose AQL carries a FROM ENDPOINT or ORGANISATION directive is never distributed: a node cannot execute it. Nothing was stored; store it without naming members in the targeting headers, and it runs federated (§12.7, §8.1, N44)"
            }
            Self::StoredQueryFanOutUnsupported => {
                "this gateway does not distribute stored-query definitions to members or report their copies, and definition.stored_query_fan_out is false: nothing was stored or read; send the request without openEHR-federation-endpoint or openEHR-federation-organisation (§12.7, N44)"
            }
            Self::StoredQueryReadOnly => {
                "this gateway's stored-query registry is read-only: its definitions are loaded from files when it starts, so nothing was stored; read or run a definition it holds (§12.7, N44)"
            }
            Self::StoredQueryReserved => {
                "the namespace eu.ferrofed.eehrxf is reserved for this gateway's own read-only stored queries, so nothing was stored; store the definition under another namespace (§12.7, N44)"
            }
            Self::ConsentDenied => {
                "the consent pre-filter does not permit asking the members that might hold this subject's EHR, and no other member holds one (N27a)"
            }
            Self::Unauthenticated => "no access token this gateway accepts (§13.1, N25)",
            Self::ScopeInsufficient => "no scope of the access token grants this operation",
            Self::PurposeOfUseRequired => "the access token declares no purpose of use (§13.4)",
            Self::AuthenticationUnavailable => "the access token cannot be verified now (§13.1)",
            Self::OperationRefused => "this gateway admits no caller to this operation",
            Self::LocalizationUnavailable => {
                "the localizer could not answer, or its exchange could not be audited, so no member was asked and where the subject has an EHR is unknown (§14.1, N4)"
            }
            Self::PatientContextMissing => {
                "only a patient/ grant covers this operation, and the access token carries no ehrId to confine it to"
            }
            Self::PatientConfinement => {
                "the request reaches beyond the patient the access token's patient/ grant is confined to, or names no patient, so nothing was sent (§5.2, §12.5)"
            }
            Self::PatientContextUnavailable => {
                "the cross-reference could not resolve the patient of the access token's patient/ grant, so the grant cannot be confined and nothing was sent (§5.2, §11.2)"
            }
            Self::SubjectUnavailable => {
                "the requested resource is not available to this request (§11.2)"
            }
            Self::Overloaded => {
                "the gateway is serving as many requests as it takes at once; retry after the time Retry-After names"
            }
            Self::RateLimited => {
                "this caller sent more requests than the gateway admits from one caller; retry after the time Retry-After names"
            }
            Self::AccessUnrecorded => {
                "the access could not be recorded, so its answer is withheld; a write may have reached the node, so read before writing again"
            }
            Self::NaturalPersonRequired => {
                "the access token names no natural person, and patient data is released to a person or for a professional the token names"
            }
            Self::AuthenticationAssuranceInsufficient => {
                "the access token states no authentication assurance at the level patient data requires"
            }
            Self::ContactPointAttributesRequired => {
                "the national contact point's access token does not carry what the eHDSI identity and treatment assertions carry"
            }
            Self::CorrelationInvalid => {
                "the national contact point's correlation header is sent once, as 1 to 128 visible ASCII characters"
            }
            Self::ConfigurationInvalid => {
                "the configuration revision is refused as config check refuses a configuration, and nothing changes"
            }
            Self::RevisionStale => {
                "the configuration revisions changed since the request read them; read them again"
            }
            Self::RevisionUnknown => "no configuration revision has that number",
            Self::RollbackUnavailable => {
                "the running configuration revision has no earlier revision to roll back to"
            }
            Self::ResourceFileOwned => {
                "the configuration file, its registry document or a care services directory provides this resource, so only that source changes it"
            }
            Self::ResourceUnknown => {
                "the newest configuration revision holds no resource of that name"
            }
            Self::ConfigurationUnmanaged => {
                "the gateway's configuration is not managed (config.ui = \"off\"), so it serves no configuration revisions"
            }
            Self::ApprovalRequired => {
                "the configuration revision changes a resource that takes the approval of an operator other than its author, and none approved it"
            }
            Self::ApprovalOwn => {
                "the author of a configuration revision cannot approve it; another operator does"
            }
            Self::ApprovalNotRequired => {
                "the configuration revision changes no resource that takes an approval"
            }
            Self::OperatorUnverified => {
                "a confirmation or an approval names its operator, and the caller is no verified operator"
            }
            Self::ConfirmationUnavailable => {
                "the configuration revision does not run awaiting a confirmation, or its deadline passed"
            }
            Self::ConfirmationPending => {
                "the running configuration revision awaits its confirmation; confirm it or roll it back first"
            }
            Self::SecretStoreUnavailable => {
                "a secret value is sent and config.secrets_dir is unset, so the gateway writes no secret file; set the secret by a reference to a file"
            }
            Self::CredentialMalformed => {
                "the request carries two credentials, or a credential field that is no credential (RFC 6750 §3.1)"
            }
            Self::EmergencyLimitReached => {
                "too many attempts for an emergency access have been made by this health professional in the window, so the access is denied (eHDSI Section II Security Services 10.0.0 §2.5)"
            }
            Self::EmergencyUncounted => {
                "the emergency access could not be counted against the health professional's limit, so it is refused; no member was asked"
            }
            Self::TrcMissing => {
                "the national contact point's access token names no patient of a treatment relationship confirmation, so no patient data is served and nothing was sent"
            }
            Self::TrcUnbound => {
                "the national contact point's request names no patient or EHR its treatment relationship confirmation binds, so nothing was sent"
            }
        }
    }
}
