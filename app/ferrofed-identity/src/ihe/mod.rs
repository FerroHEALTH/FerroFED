// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The adapters of the IHE binding (Annex A) over `ihe_iti`: each implements
//! a seam of [`role`](crate::role) or reads the registry's source.
//!
//! No specification governs the grouping: our own design.

pub mod audit;
pub mod mcsd;
pub mod pdqm;
pub mod pixm;
pub mod pmir;
pub mod xcpd;
