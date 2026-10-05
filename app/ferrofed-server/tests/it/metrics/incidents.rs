// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The integrity incident counter: each kind counts the incidents emitted of
//! it and nothing else, labelled by `kind` alone (§12b.2, N42).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use axum::body::Body;
use ferrofed_registry::incident::{Detection, Incident, Kind};
use ferrofed_server::metrics::Metrics;
use http::{Request, StatusCode};

use crate::facade::EHR_A;
use crate::metrics::{Sample, count, parse};
use crate::path_ehr_id::{answer, holder, over};

type TestResult = Result<(), Box<dyn Error>>;

/// The counter's Prometheus name.
const INCIDENTS: &str = "ferrofed_integrity_incidents_total";

/// The counter of every kind, scraped from a fresh surface: the counts are
/// the process's, so any surface reads them.
fn scraped() -> Result<Vec<Sample>, Box<dyn Error>> {
    Ok(parse(&Metrics::default().render()?)?)
}

/// The count of `kind` in `samples`, parsed.
fn of(samples: &[Sample], kind: Kind) -> Result<u64, Box<dyn Error>> {
    Ok(count(samples, INCIDENTS, &[("kind", kind.as_str())])
        .ok_or("every kind is exposed")?
        .parse()?)
}

/// One incident of `kind`, over synthetic routing ids.
fn incident(kind: Kind) -> Result<Incident, Box<dyn Error>> {
    let ehr_id = EHR_A.parse()?;
    Ok(match kind {
        Kind::LearnedCreatingSystemConflict => Incident::LearnedCreatingSystemConflict {
            creating_system_id: "cdr-x.example.org".parse()?,
            first: "node-a-pub".parse()?,
            second: "node-b-pub".parse()?,
        },
        Kind::RegisteredCreatingSystemConflict => Incident::RegisteredCreatingSystemConflict {
            creating_system_id: "cdr-x.example.org".parse()?,
            registered: "node-a".parse()?,
            learned: "node-b-pub".parse()?,
        },
        Kind::EhrIdCollision => Incident::EhrIdCollision {
            ehr_id,
            detection: Detection::AskAll,
            claimants: vec!["node-a-pub".parse()?, "node-b-pub".parse()?],
        },
        Kind::IndexInsertCollision => Incident::IndexInsertCollision {
            ehr_id,
            claimants: vec!["node-a".parse()?, "node-b".parse()?],
        },
        _ => return Err("a kind this test does not know".into()),
    })
}

#[test]
fn every_kind_is_exposed_and_counts_its_own_incidents_alone() -> TestResult {
    for kind in Kind::ALL {
        let before = scraped()?;
        incident(kind)?.emit();
        let after = scraped()?;
        for other in Kind::ALL {
            let expected = of(&before, other)? + u64::from(other == kind);
            assert_eq!(expected, of(&after, other)?, "{other} after one {kind}");
        }
    }
    for sample in scraped()?.iter().filter(|sample| sample.name == INCIDENTS) {
        assert_eq!(
            vec!["kind"],
            sample.labels.keys().collect::<Vec<_>>(),
            "kind is the only label: {sample:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_collision_the_gateway_refuses_counts_one_ehr_id_collision() -> TestResult {
    let a = holder().await;
    let b = holder().await;
    let dir = tempfile::tempdir()?;
    let app = over(dir.path(), &a, &b)?;
    let before = of(&scraped()?, Kind::EhrIdCollision)?;
    let (status, _, text) = answer(
        app,
        Request::get(format!("/v1/ehr/{EHR_A}")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    assert_eq!(before + 1, of(&scraped()?, Kind::EhrIdCollision)?);
    Ok(())
}
