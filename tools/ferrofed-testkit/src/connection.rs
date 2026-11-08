// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One HTTP/1.1 connection of a harness device served by hyper, which the
//! capturing proxy, the harness Patient Identity Registry and the
//! performance gate's token endpoint share. No specification governs the
//! harness: our own design.

use std::sync::Arc;

use bytes::Bytes;
use http::{Request, Response};
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

/// Serves `stream` on a task of its own, each request answered by `handle`
/// with the device's `shared` state.
pub(crate) fn serve<S, H, F, E>(stream: TcpStream, shared: Arc<S>, handle: H)
where
    S: Send + Sync + 'static,
    H: Fn(Arc<S>, Request<Incoming>) -> F + Send + Sync + 'static,
    F: Future<Output = Result<Response<Full<Bytes>>, E>> + Send + 'static,
    E: Into<Box<dyn std::error::Error + Send + Sync>> + 'static,
{
    tokio::spawn(async move {
        let service = service_fn(move |request| handle(Arc::clone(&shared), request));
        // A connection ends in an error whenever a client hangs up or the
        // handler refuses it; both are outcomes a test causes on purpose.
        match http1::Builder::new()
            .serve_connection(TokioIo::new(stream), service)
            .await
        {
            Ok(()) | Err(_) => {}
        }
    });
}
