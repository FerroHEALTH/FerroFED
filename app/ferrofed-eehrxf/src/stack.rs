// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The thread a mapping runs on.
//!
//! The FHIRconnect engine walks a template and a composition recursively,
//! deeper than the 2 MiB a runtime worker thread has, so every mapping the
//! gateway runs is run on a thread of its own with [`STACK`] of stack, the
//! caller's thread, a runtime worker among them, keeping its own. No
//! specification governs this: our own design.

/// The stack a template is built and a composition mapped on.
pub const STACK: usize = 16 * 1024 * 1024;

/// Why work did not run on its thread.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Deep {
    /// The thread could not be started.
    #[error("the thread a mapping runs on could not be started")]
    Thread(#[source] std::io::Error),
    /// The work panicked on its thread, a defect of the mapping engine.
    #[error("a mapping panicked on its thread")]
    Panicked,
}

/// Runs `work` on a thread of its own with [`STACK`] of stack, and waits for
/// its answer.
///
/// # Errors
///
/// [`Deep::Thread`] when the thread cannot be started, and
/// [`Deep::Panicked`] when `work` panicked.
pub fn deep<T: Send>(work: impl FnOnce() -> T + Send) -> Result<T, Deep> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name(String::from("ferrofed-mapping"))
            .stack_size(STACK)
            .spawn_scoped(scope, work)
            .map_err(Deep::Thread)?
            .join()
            .map_err(|_panicked| Deep::Panicked)
    })
}
