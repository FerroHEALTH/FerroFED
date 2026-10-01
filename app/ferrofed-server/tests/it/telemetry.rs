// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console: the rendering `auto` resolves to, both renderings, and the
//! filter, which is honoured or refused but never silently replaced.

use crate::support::{Logs, lines};
use ferrofed_server::telemetry::{DEFAULT_FILTER, Error, Format, Rendering, subscriber};
use std::error::Error as StdError;

/// Writes one `info` line through a subscriber built for `rendering` and
/// `filter`, and returns what it wrote.
fn emitted(rendering: Rendering, filter: &str) -> Result<String, Error> {
    let logs = Logs::default();
    let capture = subscriber(rendering, filter, false, logs.clone())?;
    tracing::subscriber::with_default(capture, || {
        tracing::info!(route = "/synthetic", "a line");
    });
    Ok(logs.text())
}

#[test]
fn auto_renders_json_off_a_terminal_and_pretty_on_one_and_an_explicit_choice_holds() {
    assert_eq!(Rendering::Json, Format::Auto.resolve(false));
    assert_eq!(Rendering::Pretty, Format::Auto.resolve(true));
    assert_eq!(Rendering::Json, Format::Json.resolve(true));
    assert_eq!(Rendering::Pretty, Format::Pretty.resolve(false));
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn the_json_rendering_writes_one_object_per_line() -> Result<(), Box<dyn StdError>> {
    let text = emitted(Rendering::Json, DEFAULT_FILTER)?;
    let parsed = lines(&text)?;
    assert_eq!(1, parsed.len(), "{text}");
    let line = parsed.first().ok_or("one line")?;
    assert_eq!("a line", line.message);
    assert_eq!(Some("/synthetic"), line.route.as_deref());
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn the_pretty_rendering_writes_readable_text() -> Result<(), Box<dyn StdError>> {
    let text = emitted(Rendering::Pretty, DEFAULT_FILTER)?;
    assert!(text.contains("a line"), "{text}");
    assert!(
        lines(&text).is_err(),
        "the pretty rendering is not JSON: {text}"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_filter_that_parses_is_honoured() -> Result<(), Box<dyn StdError>> {
    let text = emitted(Rendering::Json, "error")?;
    assert!(text.is_empty(), "an info line is below error: {text}");
    Ok(())
}

#[test]
fn a_filter_that_does_not_parse_is_refused() {
    assert!(
        matches!(
            emitted(Rendering::Json, "info,=,,"),
            Err(Error::Filter { .. })
        ),
        "a broken filter is refused, never replaced"
    );
}
