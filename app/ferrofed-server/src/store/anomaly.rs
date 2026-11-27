// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The count of the requests national contact points relay, per anomaly
//! rule and subject, shared by every replica.
//!
//! The `NCPeH` Architecture Specifications 10.0.0 §5.1.9 trigger each
//! anomaly warning when too many requests arrive "in a specified time
//! interval". `ferrofed.contact_point_requests` holds one row per rule and
//! subject: the rule's name, the SHA-256 digest of what it counts by
//! ([`Subject`]), so the table names no patient and no point of care, the
//! requests counted in the current window, and `expires_at`, the end of
//! that window, past which the next request opens a new one and the sweep
//! deletes the row.
//!
//! [`Store::count_requests`] counts one request against every rule in one
//! statement: an upsert per rule that adds the request to the open window,
//! or opens a new window when the last one ended, and returns each count.
//! The windows are fixed, each opened by its first request, and their
//! instants are the database's own clock, one for every replica. No
//! specification governs the table or the statement: our own design.

use ferrofed_registry::definition::store::StoreError;

use crate::config::anomaly::Rule;
use crate::store::Store;
use crate::store::connection::backend;

/// The table of the counted requests.
pub const TABLE: &str = "ferrofed.contact_point_requests";

/// Counts one request against the rules `$1`, each over the subject of the
/// same place in `$2` with a window of the seconds in `$3`, and answers each
/// rule with the requests its window now holds.
const COUNT: &str = "INSERT INTO ferrofed.contact_point_requests AS held
  (rule, subject, requests, expires_at)
SELECT rule, subject, 1, now() + make_interval(secs => window_s)
FROM unnest($1::text[], $2::text[], $3::bigint[]) AS asked(rule, subject, window_s)
ON CONFLICT (rule, subject) DO UPDATE SET
  requests = CASE WHEN held.expires_at <= now() THEN 1 ELSE held.requests + 1 END,
  expires_at = CASE WHEN held.expires_at <= now() THEN EXCLUDED.expires_at ELSE held.expires_at END
RETURNING rule, requests";

/// What one rule counts a request by, as the table keys it: the SHA-256
/// digest of the text, length-prefixed by part.
///
/// `Debug` shows the digest alone, which names no patient and no point of
/// care.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subject(String);

impl Subject {
    /// The subject of the parts `parts`.
    #[must_use]
    pub fn of(parts: &[&str]) -> Self {
        // NOTE: no specification governs this: our own design; each part is length-prefixed,
        // so no two subjects share the text the digest is taken over.
        let text = parts.iter().fold(String::new(), |mut text, part| {
            text.push_str(&part.len().to_string());
            text.push(':');
            text.push_str(part);
            text
        });
        Self(crate::digest::sha256(text.as_bytes()))
    }

    /// The digest, as the table keys the subject.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One rule a request is counted against.
#[derive(Debug, Clone)]
pub struct Counting<'a> {
    /// The rule's name, as the table keys it.
    pub rule: &'static str,
    /// What the rule counts the request by.
    pub subject: Subject,
    /// The rule.
    pub limits: &'a Rule,
}

impl Store {
    /// Counts one request against each of `rules`, in one statement, and
    /// answers each rule's name with the requests its window now holds.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when the database does not answer in time, a
    /// window does not fit the database's integer, or the statement fails:
    /// the request is then not counted.
    pub async fn count_requests(
        &self,
        rules: &[Counting<'_>],
    ) -> Result<Vec<(String, u64)>, StoreError> {
        let names: Vec<String> = rules.iter().map(|rule| rule.rule.to_owned()).collect();
        let subjects: Vec<String> = rules
            .iter()
            .map(|rule| rule.subject.as_str().to_owned())
            .collect();
        let windows = rules
            .iter()
            .map(|rule| i64::try_from(rule.limits.window.as_secs()).map_err(backend))
            .collect::<Result<Vec<i64>, StoreError>>()?;
        self.call_async(move |client| {
            Box::pin(async move {
                let rows = client
                    .query(COUNT, &[&names, &subjects, &windows])
                    .await
                    .map_err(backend)?;
                rows.iter()
                    .map(|row| {
                        let rule: String = row.try_get(0).map_err(backend)?;
                        let requests: i64 = row.try_get(1).map_err(backend)?;
                        Ok((rule, u64::try_from(requests).map_err(backend)?))
                    })
                    .collect()
            })
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::{COUNT, Subject, TABLE};

    #[test]
    fn a_subject_is_keyed_by_a_digest_that_names_no_part() {
        let subject = Subject::of(&["urn:oid:2.999.1", "Qz7-patient-1"]);
        assert_eq!(64, subject.as_str().len(), "{subject:?}");
        assert!(
            subject
                .as_str()
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        );
        assert!(!format!("{subject:?}").contains("Qz7-patient-1"));
        assert_ne!(Subject::of(&["ab", "c"]), Subject::of(&["a", "bc"]));
    }

    #[test]
    fn the_count_runs_in_one_statement_over_the_table() {
        assert!(COUNT.starts_with(&format!("INSERT INTO {TABLE} AS held")));
        assert!(COUNT.contains("ON CONFLICT (rule, subject) DO UPDATE"));
        assert!(!COUNT.contains(';'), "one statement");
    }
}
