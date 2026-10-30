// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every request body is held to a limit before anything reads it: a body
//! over the limit is `413` with or without a session, and no form runs, a
//! body that breaks off is `400` and one never finished is `408`, while an
//! import of TOML at its own limit still runs.

use std::collections::VecDeque;
use std::error::Error;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::{Body, Bytes, HttpBody};
use ferrofed_console::configure::import::IMPORT_LIMIT;
use ferrofed_console::server::{BODY_LIMIT, BODY_READ_TIMEOUT, CONFIGURE_BODY_LIMIT};
use ferrofed_console::session::COOKIE;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode};
use http_body_util::{BodyExt as _, Full};
use hyper::body::Frame;

use crate::configure::{answers, form, signed_in_console};

/// The route an import of TOML posts to, the configuration's server
/// function, which takes the larger limit.
const IMPORT: &str = ferrofed_console::act::configure::IMPORT;
use crate::support::send;

/// What revision 2 changes against the running revision 1.
const IMPORTED: &str = r#"{"revision":{"id":2,"base":1,"origin":"import","restores":null,
    "author":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},
    "at":"2026-10-15T09:01:00Z","reason":"from git","toml":""},
    "diff":{"from":1,"to":2,"changes":[]}}"#;

/// A form post of `body` to `path`, as the console's page sends one, with
/// its `Content-Length` and the session `cookie` when one is given.
fn post(path: &str, body: String, cookie: Option<&str>) -> Result<Request<Body>, Box<dyn Error>> {
    let mut request = Request::post(path)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("content-length", body.len())
        .header("sec-fetch-site", "same-origin");
    if let Some(session) = cookie {
        request = request.header("cookie", format!("{COOKIE}={session}"));
    }
    Ok(request.body(Body::from(body))?)
}

/// How many requests `listener` received.
async fn asked(listener: &Server) -> Result<usize, Box<dyn Error>> {
    Ok(listener
        .received_requests()
        .await
        .ok_or("the stub records requests")?
        .len())
}

#[tokio::test(flavor = "multi_thread")]
async fn an_oversized_body_with_no_session_is_refused_before_any_form_runs()
-> Result<(), Box<dyn Error>> {
    let listener = Server::start().await;
    let (service, _session) = signed_in_console(&listener)?;
    let toml = "#".repeat(CONFIGURE_BODY_LIMIT);
    let body = form(&[("action", "import"), ("toml", &toml), ("confirm", "yes")]);
    let (response, _) = send(&service, post(IMPORT, body, None)?).await?;
    assert_eq!(StatusCode::PAYLOAD_TOO_LARGE, response.status());
    let (response, _) = send(
        &service,
        post(IMPORT, "x".repeat(CONFIGURE_BODY_LIMIT + 1), None)?,
    )
    .await?;
    assert_eq!(StatusCode::PAYLOAD_TOO_LARGE, response.status());
    assert_eq!(0, asked(&listener).await?, "no replica is asked");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn an_oversized_body_to_any_other_route_is_refused() -> Result<(), Box<dyn Error>> {
    let listener = Server::start().await;
    let (service, session) = signed_in_console(&listener)?;
    for path in [
        "/configuration",
        "/query",
        "/integrity",
        "/stored-queries/org.example::weights",
        "/preference",
        "/logout",
    ] {
        let request = post(path, "x".repeat(BODY_LIMIT + 1), Some(session.as_str()))?;
        let (response, _) = send(&service, request).await?;
        assert_eq!(StatusCode::PAYLOAD_TOO_LARGE, response.status(), "{path}");
    }
    assert_eq!(0, asked(&listener).await?, "no replica is asked");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn an_oversized_body_sent_without_its_length_runs_no_form() -> Result<(), Box<dyn Error>> {
    let listener = Server::start().await;
    let (service, session) = signed_in_console(&listener)?;
    let request = Request::post(IMPORT)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .header("sec-fetch-site", "same-origin")
        .header("cookie", format!("{COOKIE}={}", session.as_str()))
        .body(Body::from(form(&[
            ("action", "import"),
            ("toml", &"#".repeat(CONFIGURE_BODY_LIMIT)),
            ("reason", "r"),
            ("confirm", "yes"),
        ])))?;
    let (response, _) = send(&service, request).await?;
    assert_eq!(StatusCode::PAYLOAD_TOO_LARGE, response.status());
    assert_eq!(0, asked(&listener).await?, "no replica is asked");
    Ok(())
}

// NOTE: RFC 9110 §15.5.14; a body sent chunked, its length unknown until it
// is read, is answered 413 once it grows past the limit, as a declared one is.
#[tokio::test(flavor = "multi_thread")]
async fn a_chunked_body_over_the_limit_is_413_and_runs_no_form() -> Result<(), Box<dyn Error>> {
    let listener = Server::start().await;
    let (service, session) = signed_in_console(&listener)?;
    for (path, limit) in [
        (IMPORT, CONFIGURE_BODY_LIMIT),
        ("/configuration", BODY_LIMIT),
    ] {
        let chunked = Full::new(Bytes::from(form(&[
            ("action", "import"),
            ("toml", &"#".repeat(limit)),
            ("reason", "r"),
            ("confirm", "yes"),
        ])))
        .map_frame(|frame| frame);
        assert!(
            !chunked.is_end_stream() && chunked.size_hint().exact().is_none(),
            "the body declares no length"
        );
        let request = Request::post(path)
            .header("content-type", "application/x-www-form-urlencoded")
            .header("transfer-encoding", "chunked")
            .header("sec-fetch-site", "same-origin")
            .header("cookie", format!("{COOKIE}={}", session.as_str()))
            .body(Body::new(chunked))?;
        let (response, _) = send(&service, request).await?;
        assert_eq!(StatusCode::PAYLOAD_TOO_LARGE, response.status(), "{path}");
    }
    assert_eq!(0, asked(&listener).await?, "no replica is asked");
    Ok(())
}

/// How a [`Chunked`] body ends after its chunks.
#[derive(Clone, Copy)]
enum End {
    /// The body breaks off, as when the client goes away.
    Broken,
    /// The body never ends, as one sent too slowly.
    Stalled,
}

/// A request body sent chunked: its `chunks`, then its `end`.
struct Chunked {
    chunks: VecDeque<Bytes>,
    end: End,
}

impl HttpBody for Chunked {
    type Data = Bytes;
    type Error = std::io::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        _context: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        let body = self.get_mut();
        if let Some(chunk) = body.chunks.pop_front() {
            return Poll::Ready(Some(Ok(Frame::data(chunk))));
        }
        match body.end {
            End::Broken => Poll::Ready(Some(Err(std::io::Error::other("the client went away")))),
            End::Stalled => Poll::Pending,
        }
    }
}

/// A form post to the import of `body`, as the session `session`.
fn chunked_post(session: &str, body: Chunked) -> Result<Request<Body>, Box<dyn Error>> {
    Ok(Request::post(IMPORT)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("sec-fetch-site", "same-origin")
        .header("cookie", format!("{COOKIE}={session}"))
        .body(Body::new(body))?)
}

/// The start of a confirmed import form, which the body never finishes.
fn started() -> VecDeque<Bytes> {
    VecDeque::from([Bytes::from(form(&[
        ("action", "import"),
        ("base", "1"),
        ("reason", "r"),
        ("confirm", "yes"),
    ]))])
}

// NOTE: RFC 9110 §15.5.9; a body still unread at the deadline is 408, so a
// body sent slowly holds the task no longer than the deadline.
#[tokio::test(start_paused = true)]
async fn a_body_that_never_ends_is_408_and_runs_no_form() -> Result<(), Box<dyn Error>> {
    let listener = Server::start().await;
    let (service, session) = signed_in_console(&listener)?;
    let stalled = Chunked {
        chunks: started(),
        end: End::Stalled,
    };
    let began = tokio::time::Instant::now();
    let (response, _) = send(&service, chunked_post(session.as_str(), stalled)?).await?;
    assert_eq!(StatusCode::REQUEST_TIMEOUT, response.status());
    assert!(
        began.elapsed() >= BODY_READ_TIMEOUT,
        "{:?}",
        began.elapsed()
    );
    assert_eq!(0, asked(&listener).await?, "no replica is asked");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_body_that_breaks_off_is_400_and_runs_no_form() -> Result<(), Box<dyn Error>> {
    let listener = Server::start().await;
    let (service, session) = signed_in_console(&listener)?;
    let broken = Chunked {
        chunks: started(),
        end: End::Broken,
    };
    let (response, _) = send(&service, chunked_post(session.as_str(), broken)?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, response.status());
    assert_eq!(0, asked(&listener).await?, "no replica is asked");
    Ok(())
}

// NOTE: RFC 9110 §15.5.14; a body that runs past the limit is 413 even when
// its Content-Length declares less, since the limit is held as it is read.
#[tokio::test(flavor = "multi_thread")]
async fn a_body_longer_than_its_declared_length_is_413() -> Result<(), Box<dyn Error>> {
    let listener = Server::start().await;
    let (service, session) = signed_in_console(&listener)?;
    let mut chunks = started();
    chunks.push_back(Bytes::from("x".repeat(CONFIGURE_BODY_LIMIT)));
    let long = Chunked {
        chunks,
        end: End::Broken,
    };
    let mut request = chunked_post(session.as_str(), long)?;
    request
        .headers_mut()
        .insert("content-length", http::HeaderValue::from_static("64"));
    let (response, _) = send(&service, request).await?;
    assert_eq!(StatusCode::PAYLOAD_TOO_LARGE, response.status());
    assert_eq!(0, asked(&listener).await?, "no replica is asked");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn an_import_at_its_own_limit_still_runs() -> Result<(), Box<dyn Error>> {
    let listener = Server::start().await;
    answers(&listener, ("POST", "/admin/config/import"), 201, IMPORTED).await;
    let (service, session) = signed_in_console(&listener)?;
    let toml = "#".repeat(IMPORT_LIMIT);
    let body = form(&[
        ("action", "import"),
        ("base", "1"),
        ("toml", &toml),
        ("reason", "from git"),
        ("confirm", "yes"),
    ]);
    assert!(
        body.len() > BODY_LIMIT,
        "the import is over the default limit"
    );
    let request = crate::support::answer_post(
        "/configuration/import-export",
        &body,
        Some(&session),
        &[("sec-fetch-site", "same-origin")],
    )?;
    let (response, text) = send(&service, request).await?;
    assert_eq!(StatusCode::OK, response.status(), "{text}");
    assert!(
        text.contains("Replica gateway-0 imported revision 2 as a dry run."),
        "{text}"
    );
    Ok(())
}
