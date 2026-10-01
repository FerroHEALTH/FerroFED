// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FerroFED server library: the run path the `ferrofed` binary and its
//! integration tests share.
//!
//! The binary is one; this release carries the workspace skeleton only.
#![doc(test(attr(deny(warnings))))]

use std::process::ExitCode;

/// Runs the server with the given arguments and returns the process exit code.
///
/// The configuration, the facade and the health family land with the server
/// shape; until then every invocation exits successfully having done nothing.
#[must_use]
pub fn run<I>(_args: I) -> ExitCode
where
    I: IntoIterator<Item = String>,
{
    // TODO(#29): the configuration, the facade and the run path.
    ExitCode::SUCCESS
}
