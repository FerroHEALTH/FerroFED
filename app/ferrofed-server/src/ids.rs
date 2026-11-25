// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stable logical ids the face gives a patient and the documents it
//! lists, and the sealing of the patient identifier each one stands for.
//!
//! A FHIR logical id names the same resource in every answer (FHIR R4
//! `Resource.id`), so a patient's id is derived from the patient's
//! identifier and nothing else, the HMAC-SHA-256 of `system|value`, and a
//! listed document's id from its holder and the holder's id of it, each
//! under its own key derived from `fhir.id_key`, in lowercase RFC 4648
//! base32 without padding, 52 characters, inside the 64 an id may hold. No
//! identifier can be read back from an id. To resolve an id the face was given, the
//! identifier is kept in the store sealed under a second key derived from
//! the same secret (AES-256-GCM, a random nonce, the id as associated data),
//! so a row moved to another id does not open. Each key is derived with
//! HKDF-SHA-256 (RFC 5869) under its own label. No specification governs
//! the derivation: our own design.

use aws_lc_rs::aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use aws_lc_rs::hkdf::{HKDF_SHA256, KeyType, Prk, Salt};
use aws_lc_rs::hmac;
use aws_lc_rs::rand::{SecureRandom, SystemRandom};
use data_encoding::BASE32_NOPAD;

/// The length of `fhir.id_key`, in bytes.
pub const KEY_LEN: usize = 32;

/// The HKDF label of the key a patient's id is derived under.
const PATIENT_ID: &[u8] = b"ferrofed fhir patient-id";

/// The HKDF label of the key a listed document's id is derived under.
const DOCUMENT_ID: &[u8] = b"ferrofed fhir document-id";

/// The HKDF label of the key an identifier is sealed under.
const SEAL: &[u8] = b"ferrofed fhir seal";

/// Why an id key, a seal or an opened identifier is refused, naming no
/// value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum IdError {
    /// The key is not 32 bytes of base64url.
    #[error("fhir.id_key is 32 bytes in base64url")]
    Key,
    /// The system's random source failed.
    #[error("the random source failed")]
    Random,
    /// An identifier could not be written for sealing.
    #[error("an identifier could not be written for sealing")]
    Encode,
    /// A sealed identifier did not open under the key and its id.
    #[error("a sealed identifier does not open under the key and its id")]
    Open,
}

/// The keys the face derives its ids and seals from `fhir.id_key`.
pub struct IdKeys {
    patient: hmac::Key,
    document: hmac::Key,
    seal: LessSafeKey,
    random: SystemRandom,
}

impl std::fmt::Debug for IdKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("IdKeys")
    }
}

/// A key length for HKDF's `expand`.
struct Len(usize);

impl KeyType for Len {
    fn len(&self) -> usize {
        self.0
    }
}

impl IdKeys {
    /// Derives the keys from `encoded`, `fhir.id_key` in base64url, padded
    /// or not.
    ///
    /// # Errors
    ///
    /// [`IdError::Key`] when it is not 32 bytes of base64url.
    pub fn from_base64url(encoded: &str) -> Result<Self, IdError> {
        let trimmed = encoded.trim().trim_end_matches('=');
        let secret = data_encoding::BASE64URL_NOPAD
            .decode(trimmed.as_bytes())
            .map_err(|_decode| IdError::Key)?;
        if secret.len() != KEY_LEN {
            return Err(IdError::Key);
        }
        let prk = Salt::new(HKDF_SHA256, &[]).extract(&secret);
        let patient = hmac::Key::new(hmac::HMAC_SHA256, &expand(&prk, PATIENT_ID, KEY_LEN)?);
        let document = hmac::Key::new(hmac::HMAC_SHA256, &expand(&prk, DOCUMENT_ID, KEY_LEN)?);
        let seal = UnboundKey::new(&AES_256_GCM, &expand(&prk, SEAL, AES_256_GCM.key_len())?)
            .map_err(|_unspecified| IdError::Key)?;
        Ok(Self {
            patient,
            document,
            seal: LessSafeKey::new(seal),
            random: SystemRandom::new(),
        })
    }

    /// The logical id of the patient `system`|`value`.
    #[must_use]
    pub fn patient(&self, (system, value): (&str, &str)) -> FaceId {
        tagged(&self.patient, &[system, value])
    }

    /// The logical id of the document the holder `holder` names `document`,
    /// such as a member's endpoint and its own id of the document.
    #[must_use]
    pub fn document(&self, (holder, document): (&str, &str)) -> FaceId {
        tagged(&self.document, &[holder, document])
    }

    /// Seals the patient `system`|`value` for the row of `id`: the nonce
    /// then the ciphertext with its tag.
    ///
    /// # Errors
    ///
    /// [`IdError::Random`] when no nonce can be drawn.
    pub fn seal(&self, id: &FaceId, (system, value): (&str, &str)) -> Result<Vec<u8>, IdError> {
        let mut nonce = [0_u8; NONCE_LEN];
        self.random
            .fill(&mut nonce)
            .map_err(|_unspecified| IdError::Random)?;
        let mut sealed = serde_json::to_vec(&[system, value]).map_err(|_json| IdError::Encode)?;
        self.seal
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(id.as_str().as_bytes()),
                &mut sealed,
            )
            .map_err(|_unspecified| IdError::Random)?;
        let mut row = nonce.to_vec();
        row.append(&mut sealed);
        Ok(row)
    }

    /// Opens `row`, a seal [`IdKeys::seal`] wrote for `id`, into the
    /// patient's system and value.
    ///
    /// # Errors
    ///
    /// [`IdError::Open`] when the row does not open under the key and `id`.
    pub fn open(&self, id: &FaceId, row: &[u8]) -> Result<(String, String), IdError> {
        let (nonce, sealed) = row.split_at_checked(NONCE_LEN).ok_or(IdError::Open)?;
        let nonce =
            Nonce::try_assume_unique_for_key(nonce).map_err(|_unspecified| IdError::Open)?;
        let mut sealed = sealed.to_vec();
        let opened = self
            .seal
            .open_in_place(nonce, Aad::from(id.as_str().as_bytes()), &mut sealed)
            .map_err(|_unspecified| IdError::Open)?;
        let [system, value]: [String; 2] =
            serde_json::from_slice(opened).map_err(|_json| IdError::Open)?;
        Ok((system, value))
    }
}

/// The id `key` tags `parts` with: the parts joined by `|`, each `\\` and
/// `|` inside a part escaped by a `\\`, so no two part lists give one input.
fn tagged(key: &hmac::Key, parts: &[&str]) -> FaceId {
    let joined: Vec<String> = parts
        .iter()
        .map(|part| part.replace('\\', "\\\\").replace('|', "\\|"))
        .collect();
    let tag = hmac::sign(key, joined.join("|").as_bytes());
    FaceId(BASE32_NOPAD.encode(tag.as_ref()).to_ascii_lowercase())
}

/// HKDF-expands `prk` under the label `info` into `len` bytes.
fn expand(prk: &Prk, info: &[u8], len: usize) -> Result<Vec<u8>, IdError> {
    let mut out = vec![0_u8; len];
    prk.expand(&[info], Len(len))
        .and_then(|okm| okm.fill(&mut out))
        .map_err(|_unspecified| IdError::Key)?;
    Ok(out)
}

/// A logical id the face gives, of a patient or a listed document.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FaceId(String);

impl FaceId {
    /// Reads `id`, an id a caller sent back, when it has the form the face
    /// gives: 52 characters of lowercase base32.
    #[must_use]
    pub fn parse(id: &str) -> Option<Self> {
        // NOTE: an id of another form is no id the face gave, and names nothing.
        (id.len() == 52
            && id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || (b'2'..=b'7').contains(&b)))
        .then(|| Self(id.to_owned()))
    }

    /// The id.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The `urn:uuid:` URN the id stands for: an RFC 9562 version 8 UUID of
    /// the first 16 bytes of the SHA-256 of the id, the same for one id in
    /// every answer.
    #[must_use]
    pub fn urn(&self) -> String {
        let hash = aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, self.0.as_bytes());
        let mut bytes = [0_u8; 16];
        for (byte, hashed) in bytes.iter_mut().zip(hash.as_ref()) {
            *byte = *hashed;
        }
        format!(
            "urn:uuid:{}",
            uuid::Builder::from_custom_bytes(bytes).into_uuid()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{FaceId, IdError, IdKeys};

    const KEY: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";

    #[test]
    fn a_patient_has_one_id_that_names_no_identifier() {
        let keys = IdKeys::from_base64url(KEY).unwrap();
        let id = keys.patient(("urn:oid:2.999.1", "synthetic-1"));
        assert_eq!(id, keys.patient(("urn:oid:2.999.1", "synthetic-1")));
        assert_ne!(id, keys.patient(("urn:oid:2.999.1", "synthetic-2")));
        assert_eq!(id.as_str().len(), 52);
        assert!(!id.as_str().contains("synthetic"));
        assert_eq!(FaceId::parse(id.as_str()), Some(id));
    }

    #[test]
    fn a_document_id_differs_from_a_patient_id_and_by_holder() {
        let keys = IdKeys::from_base64url(KEY).unwrap();
        assert_ne!(keys.document(("s", "v")), keys.patient(("s", "v")));
        assert_ne!(keys.document(("a|b", "c")), keys.document(("a", "b|c")));
        assert_eq!(
            keys.document(("node-a", "doc-1")),
            keys.document(("node-a", "doc-1"))
        );
        assert_ne!(
            keys.document(("node-a", "doc-1")),
            keys.document(("node-b", "doc-1"))
        );
    }

    #[test]
    fn another_key_gives_another_id() {
        let other = IdKeys::from_base64url("AQECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8").unwrap();
        let keys = IdKeys::from_base64url(KEY).unwrap();
        assert_ne!(keys.patient(("s", "v")), other.patient(("s", "v")));
    }

    #[test]
    fn a_seal_opens_under_its_id_alone() {
        let keys = IdKeys::from_base64url(KEY).unwrap();
        let id = keys.patient(("urn:oid:2.999.1", "synthetic-1"));
        let row = keys.seal(&id, ("urn:oid:2.999.1", "synthetic-1")).unwrap();
        assert!(!String::from_utf8_lossy(&row).contains("synthetic"));
        assert_eq!(
            keys.open(&id, &row).unwrap(),
            ("urn:oid:2.999.1".to_owned(), "synthetic-1".to_owned())
        );
        let other = keys.patient(("urn:oid:2.999.1", "synthetic-2"));
        assert_eq!(keys.open(&other, &row), Err(IdError::Open));
    }

    #[test]
    fn a_key_of_another_length_is_refused() {
        assert_eq!(IdKeys::from_base64url("AAEC").err(), Some(IdError::Key));
        assert_eq!(
            IdKeys::from_base64url("not base64!").err(),
            Some(IdError::Key)
        );
    }

    #[test]
    fn an_id_stands_for_one_urn() {
        let keys = IdKeys::from_base64url(KEY).unwrap();
        let id = keys.document(("summary", "patient"));
        assert_eq!(id.urn(), id.urn());
        assert_ne!(id.urn(), keys.document(("summary", "other")).urn());
        assert!(id.urn().starts_with("urn:uuid:"));
    }

    #[test]
    fn an_id_of_another_form_is_no_patient_id() {
        assert_eq!(FaceId::parse("pat-a"), None);
        assert_eq!(FaceId::parse(&"A".repeat(52)), None);
    }
}
