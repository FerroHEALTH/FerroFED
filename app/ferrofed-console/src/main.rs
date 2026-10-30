// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `ferrofed-console` binary: a thin entry point over the library run
//! path.

fn main() -> std::process::ExitCode {
    ferrofed_console::command::run(std::env::args())
}
