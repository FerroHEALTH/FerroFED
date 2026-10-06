// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The client queries of a conformance run, built through `openehr-query`.
//!
//! Each query starts from a fixed AQL template the parser reads. A value
//! reaches it bound to a parameter of the syntax tree, which the printer
//! writes as an escaped literal (AQL §Parameters); a condition is joined to
//! the tree's `WHERE` with `AND`; and a `FROM ENDPOINT` or `ORGANISATION`
//! directive is set on the tree (§8.1). The text the gateway receives is the
//! printed tree, so no value is ever spliced into AQL text.
//!
//! # Examples
//!
//! ```
//! use ferrofed_server::conformance::aql::ClientQuery;
//! use openehr_query::ast::Primitive;
//! use openehr_query::bind::Parameters;
//! use openehr_query::federation::DirectiveKind;
//!
//! let mut uid = Parameters::new();
//! uid.insert("uid", Primitive::String("it's".to_owned()));
//! let aql = ClientQuery::parse("SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c")?
//!     .and_where("SELECT c FROM COMPOSITION c WHERE c/uid/value = $uid", &uid)?
//!     .directed(DirectiveKind::Endpoint, None, &["node-a-pub"])
//!     .to_aql();
//! let expected = "SELECT c/uid/value FROM ENDPOINT ['node-a-pub'] CONTAINS EHR e \
//!                 CONTAINS COMPOSITION c WHERE c/uid/value='it\\'s'";
//! assert_eq!(expected, aql);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::fmt;

use openehr_query::ast::{Primitive, Span, WhereExpr};
use openehr_query::bind::{BindError, Parameters, bind};
use openehr_query::federation::{
    Directive, DirectiveKind, Federated, parse_federated, to_federated_aql,
};
use openehr_query::parser::{ParseError, parse_str};
use secrecy::ExposeSecret;

use crate::conformance::fixture::SyntheticPatient;

/// The patient's condition over `EHR_STATUS.subject.external_ref` for an
/// `EHR` bound to `e`, as a client writes it (§7.1).
const PATIENT: &str = "SELECT e/ehr_id/value FROM EHR e \
     WHERE e/ehr_status/subject/external_ref/id/value = $patient_id \
     AND e/ehr_status/subject/external_ref/namespace = $patient_namespace";

/// A query template could not be built into a client query.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AqlError {
    /// The template is no AQL the parser reads.
    #[error("a query template is no AQL")]
    Template(#[source] ParseError),
    /// The values could not be bound to the template's parameters.
    #[error("the values could not be bound to a query template's parameters")]
    Parameters(#[source] BindError),
    /// A condition's template has no `WHERE` clause.
    #[error("a condition's query template has no WHERE clause")]
    NoCondition,
}

/// A client query in the making: a parsed template, with the values bound
/// to its parameters, the conditions joined to it and its directive.
///
/// `Debug` shows nothing of the query, which may hold a patient identifier.
#[derive(Clone)]
pub struct ClientQuery {
    /// The query, with its directive when it has one.
    query: Federated,
}

impl ClientQuery {
    /// Parses `template`, which may carry its own directive.
    ///
    /// # Errors
    ///
    /// Returns [`AqlError::Template`] when `template` is no AQL.
    pub fn parse(template: &str) -> Result<Self, AqlError> {
        parse_federated(template)
            .map(|query| Self { query })
            .map_err(AqlError::Template)
    }

    /// Returns the query with `parameters` bound to every parameter it uses.
    ///
    /// # Errors
    ///
    /// Returns [`AqlError::Parameters`] when a parameter the query uses has
    /// no value, or a value has no parameter.
    pub fn bound(mut self, parameters: &Parameters) -> Result<Self, AqlError> {
        bind(&mut self.query.query, parameters).map_err(AqlError::Parameters)?;
        Ok(self)
    }

    /// Returns the query with the `WHERE` condition of `template` joined to
    /// its own with `AND`, `parameters` bound to the condition's parameters.
    ///
    /// # Errors
    ///
    /// Returns [`AqlError::Template`] when `template` is no AQL,
    /// [`AqlError::Parameters`] when its parameters and `parameters` differ,
    /// and [`AqlError::NoCondition`] when it has no `WHERE` clause.
    pub fn and_where(mut self, template: &str, parameters: &Parameters) -> Result<Self, AqlError> {
        let mut condition = parse_str(template).map_err(AqlError::Template)?;
        bind(&mut condition, parameters).map_err(AqlError::Parameters)?;
        let added = condition.where_.ok_or(AqlError::NoCondition)?;
        self.query.query.where_ = Some(match self.query.query.where_.take() {
            Some(own) => WhereExpr::And(Box::new(own), Box::new(added)),
            None => added,
        });
        Ok(self)
    }

    /// Returns the query with the patient's condition over
    /// `EHR_STATUS.subject.external_ref` joined to its own (§7.1).
    ///
    /// # Errors
    ///
    /// Returns [`AqlError`] when the condition cannot be built, which its
    /// fixed template never gives cause for.
    pub fn of_patient(self, patient: &SyntheticPatient) -> Result<Self, AqlError> {
        let mut parameters = Parameters::new();
        parameters.insert(
            "patient_id",
            Primitive::String(patient.value().expose_secret().to_owned()),
        );
        parameters.insert(
            "patient_namespace",
            Primitive::String(patient.namespace().to_owned()),
        );
        self.and_where(PATIENT, &parameters)
    }

    /// Returns the query directed by `FROM kind variable [ids]` (§8.1), in
    /// place of any directive it had.
    #[must_use]
    pub fn directed(mut self, kind: DirectiveKind, variable: Option<&str>, ids: &[&str]) -> Self {
        self.query.directive = Some(Directive {
            kind,
            variable: variable.map(str::to_owned),
            ids: ids.iter().map(|&id| id.to_owned()).collect(),
            span: Span::default(),
        });
        self
    }

    /// Returns the query as the AQL text a client sends, its directive in
    /// front of the containment.
    #[must_use]
    pub fn to_aql(&self) -> String {
        to_federated_aql(&self.query)
    }
}

impl fmt::Debug for ClientQuery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientQuery").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use openehr_query::ast::Primitive;
    use openehr_query::bind::Parameters;
    use openehr_query::federation::DirectiveKind;
    use openehr_query::parser::parse_str;
    use secrecy::SecretString;

    use super::{AqlError, ClientQuery};
    use crate::conformance::fixture::SyntheticPatient;

    const COMPOSITIONS: &str = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c";

    fn patient(value: &str) -> Result<SyntheticPatient, Box<dyn std::error::Error>> {
        Ok(SyntheticPatient::new(
            "urn:oid:2.999.1",
            SecretString::from(value),
        )?)
    }

    #[test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    fn the_patient_condition_reads_as_the_client_writes_it()
    -> Result<(), Box<dyn std::error::Error>> {
        let aql = ClientQuery::parse(COMPOSITIONS)?
            .of_patient(&patient("ffd-4711")?)?
            .to_aql();
        assert_eq!(
            parse_str(
                "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
                 WHERE e/ehr_status/subject/external_ref/id/value = 'ffd-4711' \
                 AND e/ehr_status/subject/external_ref/namespace = 'urn:oid:2.999.1'"
            )?,
            parse_str(&aql)?
        );
        Ok(())
    }

    #[test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    fn a_quote_in_a_value_stays_inside_its_literal() -> Result<(), Box<dyn std::error::Error>> {
        let mut parameters = Parameters::new();
        parameters.insert("uid", Primitive::String("x' OR '1' = '1".to_owned()));
        let aql = ClientQuery::parse(COMPOSITIONS)?
            .and_where(
                "SELECT c FROM COMPOSITION c WHERE c/uid/value = $uid",
                &parameters,
            )?
            .to_aql();
        let mut expected = parse_str(COMPOSITIONS)?;
        expected.where_ =
            parse_str("SELECT c FROM COMPOSITION c WHERE c/uid/value = 'x\\' OR \\'1\\' = \\'1'")?
                .where_;
        assert_eq!(
            expected,
            parse_str(&aql)?,
            "the value is one literal: {aql}"
        );
        Ok(())
    }

    #[test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    fn a_condition_joins_the_query_s_own_with_and() -> Result<(), Box<dyn std::error::Error>> {
        let aql = ClientQuery::parse(
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE c/name/value = 'a'",
        )?
        .of_patient(&patient("ffd-4711")?)?
        .to_aql();
        assert_eq!(
            parse_str(
                "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
                 WHERE c/name/value = 'a' \
                 AND (e/ehr_status/subject/external_ref/id/value = 'ffd-4711' \
                 AND e/ehr_status/subject/external_ref/namespace = 'urn:oid:2.999.1')"
            )?,
            parse_str(&aql)?
        );
        Ok(())
    }

    #[test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    fn a_directive_goes_in_front_of_the_containment() -> Result<(), Box<dyn std::error::Error>> {
        let aql = ClientQuery::parse(COMPOSITIONS)?
            .directed(
                DirectiveKind::Endpoint,
                Some("p"),
                &["node-a-pub", "node-b-pub"],
            )
            .to_aql();
        assert_eq!(
            "SELECT c/uid/value FROM ENDPOINT p ['node-a-pub', 'node-b-pub'] CONTAINS EHR e \
             CONTAINS COMPOSITION c",
            aql
        );
        let aql = ClientQuery::parse(COMPOSITIONS)?
            .directed(DirectiveKind::Organisation, None, &["org-a"])
            .to_aql();
        assert_eq!(
            "SELECT c/uid/value FROM ORGANISATION ['org-a'] CONTAINS EHR e CONTAINS COMPOSITION c",
            aql
        );
        Ok(())
    }

    #[test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    fn an_unbound_or_unknown_parameter_is_an_error_and_never_a_query() -> Result<(), AqlError> {
        let query = ClientQuery::parse("SELECT c FROM COMPOSITION c WHERE c/uid/value = $uid")?;
        assert!(matches!(
            query.bound(&Parameters::new()),
            Err(AqlError::Parameters(_))
        ));
        let empty = ClientQuery::parse(COMPOSITIONS)?;
        assert!(matches!(
            empty.and_where(COMPOSITIONS, &Parameters::new()),
            Err(AqlError::NoCondition)
        ));
        assert!(matches!(
            ClientQuery::parse("SELECT FROM"),
            Err(AqlError::Template(_))
        ));
        Ok(())
    }

    #[test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    fn debug_shows_nothing_of_the_query() -> Result<(), Box<dyn std::error::Error>> {
        let query = ClientQuery::parse(COMPOSITIONS)?.of_patient(&patient("ffd-4711")?)?;
        assert_eq!("ClientQuery { .. }", format!("{query:?}"));
        Ok(())
    }
}
