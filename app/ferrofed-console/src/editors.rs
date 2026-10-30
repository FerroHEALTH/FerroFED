// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's own components.
//!
//! Each draws a FerroUI component with what the console's own pages
//! hold. No specification governs
//! this: our own design.

use ferrohealth_ui::highlight::Language;
use ferrohealth_ui::html::Html;
use ferrohealth_ui::view::editor::{Editor, code_editor};

/// The id and the name of the query console's AQL text area.
pub const AQL: &str = "aql";

/// The query console's AQL editor: the highlighted text area FerroUI draws,
/// holding `text`, its text posted with the console's form.
///
/// The server writes the text into the text area, and the text area takes
/// typing from the first paint, so the form posts what was typed whether or
/// not the scripts run (FerroUI's interaction contract); FerroUI's `editor`
/// module colours the AQL as it is typed.
#[must_use]
pub fn aql_editor(text: String) -> Html {
    code_editor(&Editor {
        id: AQL.into(),
        language: Language::Aql,
        text,
        marks: Vec::new(),
        editable: true,
    })
}
