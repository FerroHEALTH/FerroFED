// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `ferrofed` binary: a thin entry point over the library run path.

use std::process::ExitCode;

fn main() -> ExitCode {
    ferrofed_server::command::run(std::env::args())
}
