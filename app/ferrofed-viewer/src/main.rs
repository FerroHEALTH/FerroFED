// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `ferrofed-viewer` binary: a thin entry point over the library run
//! path.

#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::process::ExitCode {
    ferrofed_viewer::command::run(std::env::args())
}

// NOTE: no specification governs this: our own design; the browser half
// starts at `ferrofed_viewer::hydrate`, so the binary is empty on wasm32.
#[cfg(target_arch = "wasm32")]
fn main() {}
