// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's own islands.
//!
//! Each is a widget of FerroUI that shares state with the console's code,
//! as small as its behaviour, as FerroUI asks; every other
//! island the console draws is one of FerroUI's ready islands.
#![expect(
    clippy::mem_forget,
    reason = "the island macro of leptos_macro 0.8 keeps each hydrated island alive by forgetting its view state, in the browser entry it writes"
)]
#![expect(
    clippy::must_use_candidate,
    reason = "the #[island] macro drops a #[must_use] from the function it writes"
)]
#![expect(
    missing_docs,
    reason = "the #[island] macro writes an undocumented struct of each island's props as they cross to the browser"
)]

use ferrohealth_ui::highlight::Language;
use ferrohealth_ui::widget::editor::{Editor, code_editor};
use leptos::prelude::*;

/// The id and the name of the query console's AQL text area.
pub const AQL: &str = "aql";

/// The query console's AQL editor: the highlighted text area FerroUI draws,
/// its text posted with the console's form.
///
/// The server writes the text into the text area, and the text area takes
/// typing from the first paint, so the form posts what was typed whether or
/// not the WebAssembly runs (FerroUI's interaction contract, layer 3); once
/// it runs, the editor colours the AQL as it is typed.
#[island]
pub fn AqlEditor(
    /// The AQL the editor opens with.
    text: String,
) -> impl IntoView {
    let text = RwSignal::new(text);
    code_editor(
        Editor {
            id: AQL.into(),
            language: Language::Aql,
            text: text.into(),
            marks: Signal::stored(Vec::new()),
            editable: Signal::stored(true),
        },
        move |typed| text.set(typed),
    )
}
