// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The toasts the server shows as a page arrives: the outcome of the
//! operator's last post, provided to the page that answers it, so the
//! toast region shows it with no WebAssembly (FerroUI R17).

use ferrohealth_ui::widget::toast::Flash;
use leptos::context::use_context;

/// The toasts a post's answer carries, provided by its handler.
#[derive(Debug, Clone, Default)]
pub struct Flashes(pub Vec<Flash>);

/// The toasts the page shows as it arrives.
#[must_use]
pub fn taken() -> Vec<Flash> {
    use_context::<Flashes>()
        .map(|flashes| flashes.0)
        .unwrap_or_default()
}
