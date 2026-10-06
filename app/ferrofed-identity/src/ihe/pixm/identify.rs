// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The reverse of a resolution: the patient a member holds under one
//! `ehr_id`, asked of the member's PIX Manager with one ITI-83 whose source
//! identifier is the `ehr_id` in the member's domain (PIXm 3.1.0
//! §2:3.83.4.1.2).
//!
//! The access log names the patient of a request addressed by `ehr_id` this
//! way (Regulation (EU) 2025/327 Art 9(1)). The `ehr_id` and the identifiers
//! the Manager answers with reach no error and no log line.

use std::time::Instant;

use ferrofed_registry::id::{EhrId, NodeId};
use ihe_iti::pixm::error::PixmError;
use ihe_iti::pixm::identifier::{CrossReference, SourceIdentifier, TargetSystem};
use secrecy::{ExposeSecret, SecretString};

use super::{PixmResolveError, PixmResolver};
use crate::ihe::audit::balp::audited_as;
use crate::role::behalf::OnBehalfOf;
use crate::role::patient::{IdentifierNamespace, PatientRef};
use crate::role::resolver::{Identification, ResolverError};

impl PixmResolver {
    /// Names the patient `member` holds under `ehr_id` by an identifier in
    /// each of `namespaces`, on behalf of `on_behalf` before `deadline`.
    pub(super) async fn identified(
        &self,
        (member, ehr_id): (&NodeId, &EhrId),
        namespaces: &[IdentifierNamespace],
        on_behalf: &OnBehalfOf,
        deadline: Instant,
    ) -> Identification {
        let Some((manager, domain)) = self.managers.iter().find_map(|manager| {
            manager
                .members
                .iter()
                .find(|(served, _)| served == member)
                .map(|(_, domain)| (manager, domain))
        }) else {
            return unavailable(PixmResolveError::UnservedMember(member.clone()));
        };
        let mut targets: Vec<(IdentifierNamespace, TargetSystem)> = Vec::new();
        for namespace in namespaces {
            match self
                .system(namespace)
                .and_then(|system| TargetSystem::new(system).ok())
            {
                Some(target) => targets.push((namespace.clone(), target)),
                None => {
                    return unavailable(PixmResolveError::UnmappedNamespace(namespace.clone()));
                }
            }
        }
        let source = match SourceIdentifier::new(
            domain.as_str().to_owned(),
            SecretString::from(ehr_id.as_str().to_owned()),
        ) {
            Ok(source) => source,
            Err(error) => return unavailable(PixmResolveError::Source(error)),
        };
        let timeout = deadline.saturating_duration_since(Instant::now());
        if timeout.is_zero() {
            return Identification::Unavailable(ResolverError::DeadlineExceeded);
        }
        let systems: Vec<TargetSystem> = targets.iter().map(|(_, target)| target.clone()).collect();
        let answer = manager
            .client
            .cross_reference(&source, &systems, &audited_as(on_behalf), timeout)
            .await;
        match answer {
            Ok(CrossReference::Matched(found)) => {
                let mut named: Vec<PatientRef> = Vec::new();
                for (namespace, target) in &targets {
                    for identifier in found.in_domain(target.as_str()) {
                        let value = identifier.value().expose_secret();
                        let repeated = named
                            .iter()
                            .any(|kept| kept.namespace() == namespace && kept.value() == value);
                        // NOTE: PIXm 3.1.0 §2:3.83.4.2.2.1: an empty value names no patient,
                        // so it is no identifier to record.
                        if let (false, Ok(patient)) = (
                            repeated,
                            PatientRef::new(namespace.clone(), SecretString::from(value)),
                        ) {
                            named.push(patient);
                        }
                    }
                }
                if named.is_empty() {
                    Identification::Unknown
                } else {
                    Identification::Named(named)
                }
            }
            Ok(CrossReference::SourceNotFound) => Identification::Unknown,
            Ok(_) => unavailable(PixmResolveError::UnexpectedAnswer),
            Err(PixmError::Timeout) => Identification::Unavailable(ResolverError::DeadlineExceeded),
            Err(error) => unavailable(PixmResolveError::Exchange(error)),
        }
    }
}

/// The patient unnamed, for `error`.
fn unavailable(error: PixmResolveError) -> Identification {
    Identification::Unavailable(ResolverError::Backend(Box::new(error)))
}
