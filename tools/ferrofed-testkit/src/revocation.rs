// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A mutual-TLS front whose server certificate a test can revoke (#1623),
//! for the revocation check of an eHDSI link (X.509 Certificates Profile
//! 7.0.0 §3.2).
//!
//! The front is [`crate::tls`]'s, over a chain of three generated at start:
//! a root CA, an intermediate CA it signs, and the front's server
//! certificate for `127.0.0.1` the intermediate signs. The intermediate
//! names the root's CRL in its CRL distribution point. The server
//! certificate names one revocation source, the [`Source`] a test picks:
//! the intermediate's CRL, the intermediate's OCSP responder in its
//! Authority Information Access extension, or either at an address no
//! connection reaches. A status server beside the front serves both CRLs
//! and answers OCSP requests (RFC 6960) signed by the intermediate, listing
//! the server certificate as good or revoked by the [`Standing`] the test
//! picks. Every key is generated at run time and never written anywhere. No
//! specification governs the harness: our own design.

use std::collections::BTreeSet;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use der::asn1::{BitString, GeneralizedTime, Ia5String};
use der::{Decode, Encode};
use rcgen::{
    BasicConstraints, CertificateParams, CertificateRevocationListParams, CrlDistributionPoint,
    CustomExtension, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyIdMethod, KeyPair,
    RevocationReason, RevokedCertParams, SerialNumber, SigningKey as _,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, Respond, ResponseTemplate};
use x509_cert::Certificate;
use x509_cert::ext::pkix::name::GeneralName;
use x509_cert::ext::pkix::{AccessDescription, AuthorityInfoAccessSyntax};
use x509_cert::name::Name;
use x509_cert::spki::AlgorithmIdentifierOwned;
use x509_ocsp::{
    BasicOcspResponse, CertStatus, OcspGeneralizedTime, OcspRequest, OcspResponse, ResponderId,
    ResponseData, RevokedInfo, SingleResponse, Version,
};

use crate::mock::{RecordingOff, Server, recorded};
use ferrohealth_server::tls_client::TlsClient;

use crate::pki::{self, Authority, PkiError};
use crate::tls::{Material, MutualTls, TlsHarnessError, thumbprint};
use crate::unreachable;

/// The serial number of the front's server certificate.
const SERVER_SERIAL: u64 = 0x21;

/// The OID of the Authority Information Access extension (RFC 5280
/// §4.2.2.1), `id-pe-authorityInfoAccess`.
const AIA: [u64; 9] = [1, 3, 6, 1, 5, 5, 7, 1, 1];

/// `id-ad-ocsp` (RFC 5280 §4.2.2.1).
const AD_OCSP: der::asn1::ObjectIdentifier =
    der::asn1::ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.48.1");

/// `ecdsa-with-SHA256` (RFC 5758 §3.2), the algorithm of the generated keys.
const ECDSA_WITH_SHA256: der::asn1::ObjectIdentifier =
    der::asn1::ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.2");

/// The one revocation source the front's server certificate names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The intermediate's CRL, in the CRL distribution point.
    Crl,
    /// The intermediate's OCSP responder, in the Authority Information
    /// Access extension.
    Ocsp,
    /// A CRL distribution point no connection reaches.
    UnreachableCrl,
    /// An OCSP responder no connection reaches.
    UnreachableOcsp,
}

/// What the revocation sources say of the front's server certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// Not revoked.
    Good,
    /// Revoked, for key compromise.
    Revoked,
}

/// Why the revocation harness could not start.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RevocationHarnessError {
    /// A certificate or a CRL could not be generated.
    #[error("the harness certificates could not be generated")]
    Certificate(#[from] rcgen::Error),
    /// A generated certificate or extension did not encode or decode.
    #[error("a harness certificate did not encode")]
    Der(#[from] der::Error),
    /// The server configuration could not be built.
    #[error("the harness TLS configuration could not be built")]
    Tls(#[from] PkiError),
    /// The front could not start.
    #[error("the harness front could not start")]
    Front(#[from] TlsHarnessError),
}

/// A running front over the revocable chain, with its status server.
#[derive(Debug)]
pub struct RevocationFront {
    front: MutualTls,
    status: Server,
}

impl RevocationFront {
    /// Starts a front on a free loopback port that passes every connection
    /// a client certificate opened through to `origin`, an
    /// `http://<host>:<port>` address with no path, its server certificate
    /// naming `source` and standing as `standing` says.
    ///
    /// # Errors
    ///
    /// A [`RevocationHarnessError`] when a certificate, a CRL, the TLS
    /// configuration or the front cannot be made.
    pub async fn start(
        origin: &str,
        source: Source,
        standing: Standing,
    ) -> Result<Self, RevocationHarnessError> {
        let status = Server::start().await;
        let base = status.uri();
        let root = Authority::generate("FerroFED harness eHDSI root CA")?;
        let intermediate = intermediate(&root, &format!("{base}/root.crl"))?;
        let server_key = KeyPair::generate()?;
        let mut params = CertificateParams::new(vec![Ipv4Addr::LOCALHOST.to_string()])?;
        params
            .distinguished_name
            .push(DnType::CommonName, "harness eHDSI link");
        params.serial_number = Some(SerialNumber::from(SERVER_SERIAL));
        params.extended_key_usages = vec![
            ExtendedKeyUsagePurpose::ServerAuth,
            ExtendedKeyUsagePurpose::ClientAuth,
        ];
        pki::validity(&mut params);
        match source {
            Source::Crl => crl_point(&mut params, &format!("{base}/intermediate.crl")),
            Source::UnreachableCrl => crl_point(
                &mut params,
                &format!("{}/intermediate.crl", unreachable::BASE),
            ),
            Source::Ocsp => ocsp_point(&mut params, &format!("{base}/ocsp"))?,
            Source::UnreachableOcsp => {
                ocsp_point(&mut params, &format!("{}/ocsp", unreachable::BASE))?;
            }
        }
        let server = params.signed_by(&server_key, &intermediate.issuer)?;
        let (client, client_key) = root.issue(
            "harness gateway client",
            Vec::new(),
            ExtendedKeyUsagePurpose::ClientAuth,
        )?;

        let revoked = match standing {
            Standing::Good => Vec::new(),
            Standing::Revoked => vec![SERVER_SERIAL],
        };
        serve_crl(&status, "/root.crl", &root.issuer, &[]).await?;
        serve_crl(&status, "/intermediate.crl", &intermediate.issuer, &revoked).await?;
        let responder = Responder {
            subject: Certificate::from_der(intermediate.certificate.der())?
                .tbs_certificate()
                .subject()
                .clone(),
            issuer: Arc::new(intermediate.issuer),
            known: BTreeSet::from([SERVER_SERIAL]),
            revoked: revoked.into_iter().collect(),
        };
        Mock::given(method("POST"))
            .and(path("/ocsp"))
            .respond_with(responder)
            .mount(&status)
            .await;

        let chain = format!("{}{}", server.pem(), intermediate.certificate.pem());
        let config = pki::server_tls_chain((&chain, &server_key), Some(&root.certificate))?;
        let material = Material {
            trust_roots: root.certificate.pem(),
            client_identity: format!("{}{}", client.pem(), client_key.serialize_pem()),
            client_thumbprint: thumbprint(client.der()),
            config,
        };
        let front = MutualTls::over(origin, &material)?;
        Ok(Self { front, status })
    }

    /// Returns the `https://127.0.0.1:<port>` origin of the front.
    #[must_use]
    pub fn origin(&self) -> String {
        self.front.origin()
    }

    /// Returns the root CA certificate, PEM, a client trusts the front by.
    #[must_use]
    pub fn trust_roots(&self) -> &str {
        self.front.trust_roots()
    }

    /// Returns the client certificate and its key, PEM, which the front
    /// admits.
    #[must_use]
    pub fn client_identity(&self) -> &str {
        self.front.client_identity()
    }

    /// Returns the gateway's TLS material for the front, read as the table
    /// at `key` reads it.
    ///
    /// # Errors
    ///
    /// [`RevocationHarnessError::Front`] when the material does not read.
    pub fn tls_client(&self, key: &str) -> Result<Arc<TlsClient>, RevocationHarnessError> {
        Ok(crate::tls::read_material(
            key,
            self.client_identity(),
            self.trust_roots(),
        )?)
    }

    /// Returns how many connections completed the handshake.
    #[must_use]
    pub fn handshakes(&self) -> usize {
        self.front.handshakes()
    }

    /// Returns how many CRL and OCSP requests the status server answered.
    ///
    /// # Errors
    ///
    /// [`RecordingOff`] when the status server records no requests.
    pub async fn status_requests(&self) -> Result<usize, RecordingOff> {
        Ok(recorded(&self.status).await?.len())
    }
}

/// An intermediate CA `root` signs, naming `crl` as its CRL distribution
/// point.
fn intermediate(root: &Authority, crl: &str) -> Result<Authority, RevocationHarnessError> {
    let key = KeyPair::generate()?;
    let mut params = CertificateParams::new(Vec::<String>::new())?;
    params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    params
        .distinguished_name
        .push(DnType::CommonName, "FerroFED harness eHDSI intermediate CA");
    params.serial_number = Some(SerialNumber::from(0x11_u64));
    pki::validity(&mut params);
    crl_point(&mut params, crl);
    let certificate = params.signed_by(&key, &root.issuer)?;
    Ok(Authority {
        certificate,
        issuer: Issuer::new(params, key),
    })
}

/// Names `url` as the one CRL distribution point of `params`.
fn crl_point(params: &mut CertificateParams, url: &str) {
    params.crl_distribution_points = vec![CrlDistributionPoint {
        uris: vec![url.to_owned()],
    }];
}

/// Names `url` as the one OCSP responder of `params`, in a non-critical
/// Authority Information Access extension.
fn ocsp_point(params: &mut CertificateParams, url: &str) -> Result<(), RevocationHarnessError> {
    let access = AuthorityInfoAccessSyntax(vec![AccessDescription {
        access_method: AD_OCSP,
        access_location: GeneralName::UniformResourceIdentifier(Ia5String::new(url)?),
    }]);
    params
        .custom_extensions
        .push(CustomExtension::from_oid_content(&AIA, access.to_der()?));
    Ok(())
}

/// Serves at `at` the CRL `issuer` signs, valid from yesterday to tomorrow,
/// listing the serial numbers `revoked`.
async fn serve_crl(
    status: &Server,
    at: &str,
    issuer: &Issuer<'static, KeyPair>,
    revoked: &[u64],
) -> Result<(), RevocationHarnessError> {
    let mut window = CertificateParams::default();
    pki::validity(&mut window);
    let crl = CertificateRevocationListParams {
        this_update: window.not_before,
        next_update: window.not_after,
        crl_number: SerialNumber::from(1_u64),
        issuing_distribution_point: None,
        revoked_certs: revoked
            .iter()
            .map(|serial| RevokedCertParams {
                serial_number: SerialNumber::from(*serial),
                revocation_time: window.not_before,
                reason_code: Some(RevocationReason::KeyCompromise),
                invalidity_date: None,
            })
            .collect(),
        key_identifier_method: KeyIdMethod::Sha256,
    }
    .signed_by(issuer)?;
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/pkix-crl")
                .set_body_bytes(crl.der().to_vec()),
        )
        .mount(status)
        .await;
    Ok(())
}

/// The OCSP responder of the intermediate: it signs each answer with the
/// intermediate's own key, so it is named by the intermediate's subject.
struct Responder {
    subject: Name,
    issuer: Arc<Issuer<'static, KeyPair>>,
    known: BTreeSet<u64>,
    revoked: BTreeSet<u64>,
}

impl Respond for Responder {
    fn respond(&self, request: &wiremock::Request) -> ResponseTemplate {
        match self.answer(&request.body) {
            Ok(body) => ResponseTemplate::new(200)
                .insert_header("content-type", "application/ocsp-response")
                .set_body_bytes(body),
            // NOTE: RFC 6960 §2.3: a request that does not decode is answered
            // malformedRequest; the harness answers a 400 a test sees as no status.
            Err(_malformed) => ResponseTemplate::new(400),
        }
    }
}

impl Responder {
    /// The DER of the signed answer to the OCSP request `body`.
    fn answer(&self, body: &[u8]) -> Result<Vec<u8>, RevocationHarnessError> {
        let request = OcspRequest::from_der(body)?;
        let now = jiff::Timestamp::now().as_second().unsigned_abs();
        let at = |seconds: u64| -> Result<OcspGeneralizedTime, der::Error> {
            GeneralizedTime::from_unix_duration(Duration::from_secs(seconds))
                .map(OcspGeneralizedTime)
        };
        let mut responses = Vec::new();
        for single in request.tbs_request.request_list {
            let serial = serial_of(single.req_cert.serial_number.as_bytes());
            let cert_status = match serial {
                Some(serial) if self.revoked.contains(&serial) => {
                    CertStatus::Revoked(RevokedInfo {
                        revocation_time: at(now.saturating_sub(86_400))?,
                        revocation_reason: None,
                    })
                }
                Some(serial) if self.known.contains(&serial) => CertStatus::good(),
                _ => CertStatus::unknown(),
            };
            responses.push(SingleResponse {
                cert_id: single.req_cert,
                cert_status,
                this_update: at(now.saturating_sub(60))?,
                next_update: Some(at(now.saturating_add(3_600))?),
                single_extensions: None,
            });
        }
        let data = ResponseData {
            version: Version::V1,
            responder_id: ResponderId::ByName(self.subject.clone()),
            produced_at: at(now)?,
            responses,
            response_extensions: None,
        };
        let signature = self.issuer.key().sign(&data.to_der()?)?;
        let basic = BasicOcspResponse {
            tbs_response_data: data,
            signature_algorithm: AlgorithmIdentifierOwned {
                oid: ECDSA_WITH_SHA256,
                parameters: None,
            },
            signature: BitString::from_bytes(&signature)?,
            certs: None,
        };
        Ok(OcspResponse::successful(basic)?.to_der()?)
    }
}

/// The serial number whose big-endian bytes are `bytes`, when it fits a
/// `u64`.
fn serial_of(bytes: &[u8]) -> Option<u64> {
    let bytes = match bytes {
        [0, rest @ ..] => rest,
        all => all,
    };
    if bytes.len() > 8 {
        return None;
    }
    Some(
        bytes
            .iter()
            .fold(0_u64, |serial, byte| (serial << 8) | u64::from(*byte)),
    )
}
