// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The onward grants the gateway runs itself, beside the client of
//! `ferrohealth-smart` every OAuth 2.0 grant goes through (§13.1, N25).
//!
//! [`lane`] reaches a node under the token `ferrohealth-smart`'s onward
//! client obtains for each verified caller, over what [`on_behalf`] hands
//! it. A regional binding's grants sit in a folder of their own behind its
//! feature: `nl` holds the Nuts grant of Annex B §B.4. No specification
//! governs the grouping: our own design.

#[cfg(feature = "nl")]
mod cache;
pub mod lane;
#[cfg(feature = "nl")]
pub mod nl;
pub mod on_behalf;
