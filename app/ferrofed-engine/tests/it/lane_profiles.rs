// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The onward lane's `federation` routes under each example profile, the
//! standards-only profile first, then Keycloak's standard exchange and
//! Entra ID's on-behalf-of grant: a partner's professional is exchanged
//! with the caller's token alone as the subject, an organisation reaches
//! the node read-only, the onward check refuses another professional,
//! another authorization server and an unmapped subject issuer, a chained
//! organisation without its correlation claim is refused, a refused
//! exchange fails the call, and no caller is ever sent the gateway's own
//! token (§13.1, N25; RFC 8693 §2.1, RFC 7523 §2.1).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ferrofed_engine::conveyance::Conveyance;
use ferrofed_engine::conveyance::caller::Principal;
use ferrofed_engine::hygiene::Withheld;
use ferrofed_engine::onward::grant::exchange::OnBehalf as _;
use ferrofed_engine::onward::grant::lane::{Lane, LaneError, Organisations};
use ferrofed_testkit::mock::Server;
use ferrohealth_smart::client::onward::{OnwardError, Rejected};
use ferrohealth_smart::federation::{Attribute, External, Field};
use ferrohealth_smart::kind::{CallerKind, IssuerKind};
use openehr_its::rest::client::Credentials;
use secrecy::{ExposeSecret as _, SecretString};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::lane::{NODE, OWN_TOKEN, refuses};
use crate::lane_checked::{
    CALLER_TOKEN, CLIENT_ID, FEDERATION, HP_IDENTIFIER, PARTNER, PERSON, conveyance,
    federation_token, forms, issuing, jws, keycloak_keys, lane_carrying, now, partner_call, sent,
};

type TestResult = Result<(), Box<dyn Error>>;

/// The example profiles, the standards-only one first.
const PROFILES: [&str; 3] = ["rfc9068", "keycloak", "entra"];

/// The correlation claim of a chained route.
const CLAIM: &str = "txn";

/// The partner's organisation, as its issuer names it.
const ORGANISATION: &str = "synthetic-partner-organisation";

/// The form parameters a token request may carry: the grant, the caller's
/// token, the target and the gateway's own client, and nothing that states
/// an attribute of the caller.
const PARAMETERS: [&str; 12] = [
    "grant_type",
    "subject_token",
    "subject_token_type",
    "requested_token_type",
    "audience",
    "resource",
    "scope",
    "client_id",
    "client_secret",
    "assertion",
    "requested_token_use",
    "client_assertion_type",
];

/// What an issuer's table sets beside `profile`'s preset: the node as its
/// indicator, Keycloak's service-account subjects, or Entra ID's scope.
#[expect(
    clippy::disallowed_types,
    reason = "the claims and tables a test token and lane carry are written as JSON"
)]
fn profile_keys(profile: &str) -> serde_json::Value {
    match profile {
        "keycloak" => keycloak_keys(),
        "entra" => json!({"scope": "api://cdr-a/user_impersonation"}),
        _ => json!({"indicators": [NODE]}),
    }
}

/// The lane of the node's `federation` issuer under `profile`, trusted to
/// map the callers of `exchanges_from`, carrying `organisations`, on a
/// route that carries `correlation` when one is given: a lane that opts in
/// reads the organisation's identifier, and one that does not reads its
/// professionals as IHE IUA.
fn federation_lane(
    server: &Server,
    profile: &str,
    exchanges_from: &str,
    (organisations, correlation): (Organisations, Option<&str>),
) -> Result<Lane, Box<dyn Error>> {
    let federation = match organisations {
        Organisations::ReadOnly => json!({
            "contact_point": false,
            "exchanges_from": [exchanges_from],
            "person_mapping": "hp_identifier",
            "countries": ["NL"],
            "issuing_authorities": ["synthetic-authority"],
            "provider_identifier_prefixes": ["urn:oid:2.999"],
            "fields": {
                "provider.healthcare_provider_identifier": [{"claim": "organisation_id"}],
            },
            "attests_authorization_details": true,
            "authorization_details_type": "organisation_access",
            "organisation_types": ["organisation_access"],
        }),
        _ => json!({
            "contact_point": false,
            "mapping": "iua",
            "exchanges_from": [exchanges_from],
            "person_mapping": "hp_identifier",
            "countries": ["NL"],
            "issuing_authorities": ["synthetic-authority"],
            "provider_identifier_prefixes": ["urn:oid:2.999."],
            "fields": {
                "professional.country_code": [{"constant": "NL"}],
                "professional.issuing_authority_name": [{"constant": "synthetic-authority"}],
            },
        }),
    };
    let mut section = json!({
        "issuer": FEDERATION, "kind": "federation", "profile": profile,
        "correlation_claim": correlation.unwrap_or_default(),
        "federation": federation,
        "assurance": {"levels": {"urn:acr:high": "high"}},
    });
    if let (Some(object), serde_json::Value::Object(extra)) =
        (section.as_object_mut(), profile_keys(profile))
    {
        object.extend(extra);
    }
    lane_carrying(server, IssuerKind::Federation, section, organisations)
}

/// `claims` with what every example profile requires of an issued token:
/// the gateway as `azp`, Entra ID's `tid`, `uti` and `ver`, and its marker
/// of a token issued for a person (`scp`) or for an application alone.
#[expect(
    clippy::disallowed_types,
    reason = "the claims and tables a test token and lane carry are written as JSON"
)]
fn for_every_profile(mut claims: serde_json::Value, person: bool) -> serde_json::Value {
    if let Some(object) = claims.as_object_mut() {
        if person {
            object.insert("scp".to_owned(), "user_impersonation".into());
            object.insert("acct".to_owned(), 0.into());
        } else {
            object.insert("idtyp".to_owned(), "app".into());
        }
        object.insert("azp".to_owned(), CLIENT_ID.into());
        object.insert("tid".to_owned(), "synthetic-tenant".into());
        object.insert("uti".to_owned(), "synthetic-uti-1".into());
        object.insert("ver".to_owned(), "2.0".into());
    }
    claims
}

/// The names of the parameters of the form-encoded `form`.
fn names(form: &str) -> Vec<String> {
    url::form_urlencoded::parse(form.as_bytes())
        .map(|(name, _)| name.into_owned())
        .collect()
}

/// The `sub` of the compact JWS `token`.
#[expect(
    clippy::disallowed_types,
    reason = "the claims and tables a test token and lane carry are written as JSON"
)]
fn subject_of(token: &str) -> Result<String, Box<dyn Error>> {
    let payload = token.split('.').nth(1).ok_or("a compact JWS")?;
    let claims: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload)?)?;
    Ok(claims
        .get("sub")
        .and_then(serde_json::Value::as_str)
        .ok_or("a sub")?
        .to_owned())
}

/// A partner's professional is exchanged at the node's `federation` issuer
/// under each profile: the caller's token is the only statement about the
/// caller the gateway sends, and the issued token, whose attributes the
/// authorization server took from it and which names no person as `sub`,
/// is what the node is sent.
#[tokio::test]
async fn a_partners_professional_is_exchanged_under_each_profile() -> TestResult {
    for profile in PROFILES {
        let issued = for_every_profile(federation_token(HP_IDENTIFIER), true);
        let server = issuing(&issued).await;
        let lane = federation_lane(&server, profile, PARTNER, (Organisations::Refused, None))?;

        let token = sent(&lane, &partner_call()?).await?;
        assert_eq!(jws(&issued), token, "{profile}");
        assert_ne!(PERSON, subject_of(&token)?, "{profile}: no person sub");
        let forms = forms(&server).await?;
        let form = forms.first().ok_or("an exchange")?;
        assert!(
            form.contains(&format!("subject_token={CALLER_TOKEN}"))
                || form.contains(&format!("assertion={CALLER_TOKEN}")),
            "{profile}: {form}"
        );
        for name in names(form) {
            assert!(
                PARAMETERS.contains(&name.as_str()),
                "{profile}: the gateway stated {name}: {form}"
            );
        }
    }
    Ok(())
}

/// Under each profile, an issued token that maps another professional, or
/// that another authorization server issued, is refused before use.
#[tokio::test]
async fn the_onward_check_holds_under_each_profile() -> TestResult {
    for profile in PROFILES {
        let server = issuing(&for_every_profile(
            federation_token("synthetic-hp-0099"),
            true,
        ))
        .await;
        let lane = federation_lane(&server, profile, PARTNER, (Organisations::Refused, None))?;
        let (refused, account) = refuses(&lane, &partner_call()?, |error| {
            matches!(
                error,
                LaneError::Onward(OnwardError::Rejected(Rejected::PersonMapping))
            )
        })
        .await?;
        assert!(refused, "{profile}: {account}");

        let mut elsewhere = for_every_profile(federation_token(HP_IDENTIFIER), true);
        elsewhere["iss"] = json!("https://as.elsewhere.example.org");
        let server = issuing(&elsewhere).await;
        let lane = federation_lane(&server, profile, PARTNER, (Organisations::Refused, None))?;
        let (refused, account) = refuses(&lane, &partner_call()?, |error| {
            matches!(
                error,
                LaneError::Onward(OnwardError::Rejected(Rejected::Issuer))
            )
        })
        .await?;
        assert!(refused, "{profile}: {account}");
    }
    Ok(())
}

/// Under each profile, a subject whose issuer is outside the federation
/// issuer's `exchanges_from` is refused before anything is sent.
#[tokio::test]
async fn an_unmapped_subject_issuer_is_refused_under_each_profile() -> TestResult {
    for profile in PROFILES {
        let server = issuing(&for_every_profile(federation_token(HP_IDENTIFIER), true)).await;
        let lane = federation_lane(
            &server,
            profile,
            "https://as.another-party.example.org",
            (Organisations::Refused, None),
        )?;
        let (refused, account) = refuses(&lane, &partner_call()?, |error| {
            matches!(
                error,
                LaneError::Onward(OnwardError::Rejected(Rejected::NotMapped))
            )
        })
        .await?;
        assert!(refused, "{profile}: {account}");
        assert!(forms(&server).await?.is_empty(), "{profile}: nothing sent");
    }
    Ok(())
}

/// The partner's organisation, whose token names the gateway in `aud` and
/// carries the correlation claim when `correlated`.
fn organisation_call(correlated: bool) -> Result<Conveyance, Box<dyn Error>> {
    let fields = BTreeMap::from([(
        Field::ProviderHealthcareProviderIdentifier,
        Attribute::Text("urn:oid:2.999.7".to_owned()),
    )]);
    let kind = CallerKind::ExternalOrganisation(Box::new(External::new(ORGANISATION, fields)));
    let mut claims = json!({
        "iss": PARTNER, "sub": ORGANISATION, "aud": CLIENT_ID, "client_id": ORGANISATION,
        "iat": now(), "exp": now() + 600,
    });
    if correlated && let Some(object) = claims.as_object_mut() {
        object.insert(CLAIM.to_owned(), "synthetic-correlation-0815".into());
    }
    conveyance(PARTNER, kind, &claims)
}

/// The organisation token the `federation` issuer issues.
#[expect(
    clippy::disallowed_types,
    reason = "the claims and tables a test token and lane carry are written as JSON"
)]
fn organisation_token(correlated: bool) -> serde_json::Value {
    let mut claims = for_every_profile(
        json!({
            "iss": FEDERATION, "sub": CLIENT_ID, "client_id": CLIENT_ID, "aud": NODE,
            "jti": "synthetic-issued-3", "iat": now(), "exp": now() + 300,
            "organisation_id": "urn:oid:2.999.7",
            "authorization_details": [{"type": "organisation_access", "locations": [NODE]}],
        }),
        false,
    );
    if correlated && let Some(object) = claims.as_object_mut() {
        object.insert(CLAIM.to_owned(), "synthetic-correlation-0815".into());
    }
    claims
}

/// Under the standards-only profile and Keycloak, a lane that opts in sends
/// the node the issued organisation token, read-only; Entra ID's
/// on-behalf-of grant carries a user alone, so there the organisation is
/// refused before anything is sent. On a chained route an organisation
/// whose token carries no correlation claim is refused before anything is
/// sent.
#[tokio::test]
async fn an_organisation_reaches_the_node_read_only_under_each_profile() -> TestResult {
    for profile in PROFILES {
        let issued = organisation_token(false);
        let server = issuing(&issued).await;
        let lane = federation_lane(&server, profile, PARTNER, (Organisations::ReadOnly, None))?;
        let call = organisation_call(false)?;
        assert!(lane.read_only(&call), "{profile}: read-only");
        if profile == "entra" {
            let (refused, account) = refuses(&lane, &call, |error| {
                matches!(error, LaneError::Onward(OnwardError::UserPrincipalRequired))
            })
            .await?;
            assert!(refused, "{profile}: {account}");
            assert!(forms(&server).await?.is_empty(), "{profile}: nothing sent");
        } else {
            assert_eq!(jws(&issued), sent(&lane, &call).await?, "{profile}");
        }

        let server = issuing(&organisation_token(true)).await;
        let lane = federation_lane(
            &server,
            profile,
            PARTNER,
            (Organisations::ReadOnly, Some(CLAIM)),
        )?;
        let (refused, account) =
            refuses(&lane, &organisation_call(false)?, |error| match profile {
                "entra" => matches!(error, LaneError::Onward(OnwardError::UserPrincipalRequired)),
                _ => matches!(error, LaneError::Onward(OnwardError::NoCorrelation)),
            })
            .await?;
        assert!(refused, "{profile}: {account}");
        assert!(forms(&server).await?.is_empty(), "{profile}: nothing sent");
    }
    Ok(())
}

/// Under each profile, a token endpoint that refuses the exchange fails the
/// call with its status, and the gateway's own token never stands in.
#[tokio::test]
async fn a_refused_exchange_fails_the_call_under_each_profile() -> TestResult {
    for profile in PROFILES {
        let server = Server::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(400).set_body_json(json!({"error": "invalid_grant"})),
            )
            .mount(&server)
            .await;
        let lane = federation_lane(&server, profile, PARTNER, (Organisations::Refused, None))?;
        let (refused, account) = refuses(&lane, &partner_call()?, |error| {
            matches!(error, LaneError::Onward(error) if error.status() == Some(http::StatusCode::BAD_REQUEST))
        })
        .await?;
        assert!(refused, "{profile}: {account}");
        assert_eq!(1, forms(&server).await?.len(), "{profile}: one exchange");
    }
    Ok(())
}

/// Under each profile, a system caller is refused and never sent the
/// gateway's own token, and only the gateway's own request carries it.
#[tokio::test]
async fn only_the_gateways_own_request_carries_its_token_under_each_profile() -> TestResult {
    for profile in PROFILES {
        let server = issuing(&for_every_profile(federation_token(HP_IDENTIFIER), true)).await;
        let lane = federation_lane(&server, profile, PARTNER, (Organisations::ReadOnly, None))?;
        let system = conveyance(
            PARTNER,
            CallerKind::System,
            &json!({"iss": PARTNER, "sub": "synthetic-system", "aud": CLIENT_ID}),
        )?;
        let (refused, account) = refuses(&lane, &system, |error| {
            matches!(
                error,
                LaneError::NotCarried {
                    caller: "system",
                    ..
                }
            )
        })
        .await?;
        assert!(refused, "{profile}: {account}");

        let withheld = Arc::new(Withheld::new(Vec::<SecretString>::new()));
        let own = lane
            .provider(
                &Conveyance::new(crate::conveyed::shared(), Principal::Gateway),
                &withheld,
            )
            .credentials()
            .await?;
        let Credentials::Bearer(own) = own else {
            return Err("a bearer token".into());
        };
        assert_eq!(OWN_TOKEN, own.expose_secret(), "{profile}");
        assert!(forms(&server).await?.is_empty(), "{profile}: nothing sent");
    }
    Ok(())
}
