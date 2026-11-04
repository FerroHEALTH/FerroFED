// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Lists the pairs of top-level modules of a crate that import each other
//! outside tests, for the module-cycle guard.
//!
//! No specification governs this: our own design (#1580).
//! `scripts/checks/module-cycles.sh` runs this binary and fails on any pair.
//!
//! The output is one line per pair, sorted: the two module names, a space
//! between them. `--edges` prints every edge `from to` instead.
//!
//! # Examples
//!
//! ```text
//! module-cycles app/ferrofed-server/src          # the mutual pairs
//! module-cycles --edges app/ferrofed-server/src  # every edge
//! ```

mod graph;

use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::{Result, bail};

fn main() -> Result<()> {
    let mut all = false;
    let mut src = None;
    for arg in std::env::args_os().skip(1) {
        if arg == "--edges" {
            all = true;
        } else if arg.to_str().is_some_and(|text| text.starts_with("--")) || src.is_some() {
            bail!("usage: module-cycles [--edges] SRC_DIR");
        } else {
            src = Some(PathBuf::from(arg));
        }
    }
    let Some(src) = src else {
        bail!("usage: module-cycles [--edges] SRC_DIR");
    };
    let edges = graph::edges(&src)?;
    let lines = if all {
        edges.into_iter().collect::<Vec<_>>()
    } else {
        graph::mutual(&edges)
    };
    let mut out = io::stdout().lock();
    for (from, to) in lines {
        writeln!(out, "{from} {to}")?;
    }
    out.flush()?;
    Ok(())
}
