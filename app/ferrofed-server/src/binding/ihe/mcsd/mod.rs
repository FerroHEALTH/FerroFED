// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mCSD care services directory (§15.1, Annex A.5): the registry the
//! gateway keeps in step with the directory ([`registry`]), from the
//! `[registry.mcsd]` settings [`crate::config::ihe::mcsd`] resolves.

pub mod registry;
