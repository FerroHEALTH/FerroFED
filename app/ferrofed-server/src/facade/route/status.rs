// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The §11.1 status of a node a request was sent to, read from its answer
//! or from the failure that left it without one.
//!
//! The access record of a routed request and the node request metrics both
//! read a node's answer with these, so the two never name one answer apart.

use ferrofed_engine::single_node::forward::ForwardError;
use http::StatusCode;
use openehr_federation::status::EndpointStatus;

/// The §11.1 status of a node that answered one request with `status`,
/// `refused` on consent grounds or not: `consent-denied` for a consent
/// refusal, `active` for a success and `node-error` for any other answer, a
/// `4xx` included.
// NOTE: §11.1, N16, N27: a node reached that answered with a failure is `node-error`, never
// `active`, and one that refused on consent grounds is `consent-denied`.
pub(crate) fn answered(status: StatusCode, refused: bool) -> EndpointStatus {
    if refused {
        EndpointStatus::ConsentDenied
    } else if status.is_success() {
        EndpointStatus::Active
    } else {
        EndpointStatus::NodeError
    }
}

/// The §11.1 status of a node that gave no answer of its own to a request
/// that ended in `failure`: `time-out` when no answer came in time or the
/// request never got a slot before its deadline, `offline` when the node
/// could not be reached, and `node-error` otherwise.
pub(crate) fn unanswered(failure: &ForwardError) -> EndpointStatus {
    match failure {
        ForwardError::TimeOut { .. } | ForwardError::Expired { .. } | ForwardError::Capped(_) => {
            EndpointStatus::TimeOut
        }
        ForwardError::Unreachable { .. } => EndpointStatus::Offline,
        _ => EndpointStatus::NodeError,
    }
}

#[cfg(test)]
mod tests {
    use http::StatusCode;
    use openehr_federation::status::EndpointStatus;

    use super::answered;

    #[test]
    fn a_node_answer_that_is_no_success_is_a_node_error_a_4xx_included() {
        for status in [StatusCode::OK, StatusCode::CREATED, StatusCode::NO_CONTENT] {
            assert_eq!(EndpointStatus::Active, answered(status, false));
        }
        for status in [
            StatusCode::NOT_FOUND,
            StatusCode::CONFLICT,
            StatusCode::PRECONDITION_FAILED,
            StatusCode::UNPROCESSABLE_ENTITY,
            StatusCode::INTERNAL_SERVER_ERROR,
        ] {
            assert_eq!(
                EndpointStatus::NodeError,
                answered(status, false),
                "§11.1, N16: {status}"
            );
        }
        assert_eq!(
            EndpointStatus::ConsentDenied,
            answered(StatusCode::FORBIDDEN, true),
            "N27"
        );
    }
}
