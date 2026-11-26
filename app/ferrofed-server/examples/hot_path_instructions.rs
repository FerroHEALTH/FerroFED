// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Runs one hot path's in-process work once, for an instruction count under
//! callgrind (`scripts/perf/instructions.sh`, #2003).
//!
//! The paths are the analysis and node rewrite of the façade's patient query
//! (`aql_rewrite`, §4, every federated query), and the merge of two members'
//! ordered answers of 200 rows each into one result set (`merge`, §9, every
//! answer more than one member gave). Only the `measured` call is counted.

use std::error::Error;

use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_federation::aql::{Analysis, Context, Paging, Targeting, analyse};
use openehr_federation::merge::{NodeAnswer, merge};
use openehr_federation::order::{Direction, ResultOrder, SortKey};
use openehr_its::rest::generated::query::ResultSetRow;
use openehr_query::bind::Parameters;

/// The namespace the patient's identifier is issued in.
const NAMESPACE: &str = "urn:oid:2.999.1";

/// The `ehr_id` the rewrite scopes the node query to.
const EHR_ID: &str = "2222aaaa-2222-4222-8222-222222222222";

/// The façade query of §7.2: the patient identified the openEHR way.
const QUERY: &str = "SELECT c/uid/value, c/context/start_time/value \
     FROM EHR e CONTAINS COMPOSITION c \
     CONTAINS OBSERVATION o[openEHR-EHR-OBSERVATION.blood_pressure.v2] \
     WHERE e/ehr_status/subject/external_ref/id/value = 'synthetic-patient-0001' \
     AND e/ehr_status/subject/external_ref/namespace = 'urn:oid:2.999.1' \
     ORDER BY c/context/start_time/value DESC";

/// The rows each member answers the merge with.
const ROWS: usize = 200;

/// Runs `path` and keeps its result opaque to the optimizer.
#[inline(never)]
fn measured<R>(path: impl FnOnce() -> R) -> R {
    std::hint::black_box(path())
}

/// The rows of the member `member`: a composition uid and a start time,
/// ordered by start time descending.
fn rows(member: usize) -> Vec<ResultSetRow> {
    (0..ROWS)
        .rev()
        .map(|index| {
            vec![
                serde_json::json!(format!(
                    "8849182c-82ad-4088-a07f-{index:012x}::cdr-{member}.example.org::1"
                )),
                serde_json::json!(format!("2026-10-{:02}T09:00:00Z", 1 + index % 28)),
            ]
        })
        .collect()
}

/// Analyses the façade query and writes its node query for [`EHR_ID`].
fn rewrite() -> Result<String, Box<dyn Error>> {
    let context = Context::new(Targeting::AskAll).with_default_namespace(NAMESPACE);
    let Analysis::Patient(query) =
        analyse(QUERY, &Parameters::default(), Paging::default(), &context)?
    else {
        return Err("the façade query names a patient".into());
    };
    Ok(query.for_node(&HierObjectId::new(EHR_ID)?).aql().to_owned())
}

fn main() -> Result<(), Box<dyn Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("name a path: aql_rewrite or merge")?;
    match path.as_str() {
        "aql_rewrite" => {
            measured(rewrite)?;
        }
        "merge" => {
            let order =
                ResultOrder::new(vec![SortKey::new(1, Direction::Descending)], vec![0], None);
            let answers = vec![
                NodeAnswer::new("node-a-pub", rows(0)),
                NodeAnswer::new("node-b-pub", rows(1)),
            ];
            measured(|| merge(answers, &order));
        }
        other => return Err(format!("unknown path `{other}`").into()),
    }
    Ok(())
}
