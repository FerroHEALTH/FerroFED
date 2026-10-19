// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The pieces every part of the configuration page is drawn from: a card's
//! head with its one line and its "Learn more" link, a field with the place
//! its refusal is shown, a confirmation step in a popover, a repeatable row
//! and the button that adds one, and the table of what a revision changes.
//!
//! The page's script finds them by their marks alone: `data-add` on a
//! button that adds a row from the `template` it names, `data-remove` on
//! one that removes the row it names, `data-slot` on a form whose answer
//! shows in that card, and `data-field-error` on a field's refusal line.

use ferrofed_registry::revision::{Difference, RevisionId};
use ferrohealth_ui::icon::{Glyph, icon};
use ferrohealth_ui::table::view::Column;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::button::{Variant, label};
use ferrohealth_ui::view::form::{ERROR, Field, INPUT, action_field, described_by, error_id};
use ferrohealth_ui::view::status::badge;
use leptos::prelude::*;
use leptos::server_fn::ServerFn as _;

use crate::configure::act::Configure;
use crate::configure::render::Loaded;

/// The book page that explains the configuration page.
pub(crate) const LEARN: &str = "https://ferrofed.eu/docs/operate/configuration-revisions.html";

/// One line of explanation and a link to the book's section `anchor`.
pub(crate) fn lede(text: &'static str, anchor: &'static str) -> AnyView {
    view! {
        <p class="fui-muted">
            {text} " " <a href=format!("{LEARN}#{anchor}") rel="external">
                "Learn more"
            </a>
        </p>
    }
    .into_any()
}

/// A card's head: its heading `title` with the id `id`, the one line
/// `line` explaining it with the book's `anchor`, and `actions` at its end.
pub(crate) fn head(
    (id, title): (&'static str, String),
    (line, anchor): (&'static str, &'static str),
    actions: impl IntoView,
) -> impl IntoView {
    view! {
        <div class="fui-split">
            <div>
                <h2 id=id class="fui-card-title">
                    {title}
                </h2>
                {lede(line, anchor)}
            </div>
            <div class="fui-cluster">{actions}</div>
        </div>
    }
}

/// A hidden field `name` of a form, holding `value`.
pub(crate) fn hidden(name: &str, value: String) -> AnyView {
    view! { <input type="hidden" name=format!("form[{name}]") value=value /> }.into_any()
}

/// The line under the field `id` where its refusal shows, empty and hidden
/// until the page marks it.
pub(crate) fn error_line(id: &str) -> impl IntoView + use<> {
    view! { <p id=error_id(id) class=ERROR data-field-error="" hidden></p> }
}

/// What a text field shows and takes beside its value.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Text {
    /// The input's type: `text` when empty.
    pub(crate) kind: &'static str,
    /// Whether the form needs it.
    pub(crate) required: bool,
    /// Whether it shows a value the form cannot change.
    pub(crate) readonly: bool,
    /// Whether it holds an identifier, set in the monospace stack.
    pub(crate) mono: bool,
    /// Its one line of help.
    pub(crate) hint: Option<&'static str>,
    /// The id of the `datalist` that offers its values.
    pub(crate) list: Option<&'static str>,
}

/// A labelled text field: its label, the input `id` named `name` (the whole
/// name, such as `form[node]`) holding
/// `value`, its line of help and the line where its refusal shows, both
/// named by the input's `aria-describedby`.
pub(crate) fn text((id, name): (&str, &str), label: String, value: String, shown: Text) -> AnyView {
    let described = described_by(id, shown.hint.is_some(), true);
    let kind = if shown.kind.is_empty() {
        "text"
    } else {
        shown.kind
    };
    let class = if shown.mono {
        format!("{INPUT} fui-mono")
    } else {
        String::from(INPUT)
    };
    let input = view! {
        <input
            type=kind
            id=id.to_owned()
            name=name.to_owned()
            value=value
            class=class
            autocomplete="off"
            spellcheck="false"
            list=shown.list
            required=shown.required
            readonly=shown.readonly
            aria-describedby=described
        />
    };
    Field::new(id, label)
        .help_if(shown.hint)
        .after(error_line(id))
        .view(input)
}

/// The breadcrumb trail from the tab `(label, href)` to the form or view
/// titled `current`, the current one last (WAI-ARIA Authoring Practices, the
/// breadcrumb pattern).
pub(crate) fn trail((label, href): (&'static str, &'static str), current: String) -> AnyView {
    view! {
        <nav class="fui-crumbs" aria-label="Breadcrumb">
            <ol>
                <li>
                    <a href=href>{label}</a>
                    <span class="fui-crumb-sep" aria-hidden="true">
                        "/"
                    </span>
                </li>
                <li aria-current="page">{current}</li>
            </ol>
        </nav>
    }
    .into_any()
}

/// The reason field every change ends with.
pub(crate) fn reason() -> AnyView {
    text(
        (crate::configure::plan::REASON, "form[reason]"),
        String::from("Reason"),
        String::new(),
        Text {
            required: true,
            hint: Some("The history records it with the revision."),
            ..Text::default()
        },
    )
    .into_any()
}

/// A form posting to the configuration's server function, its answer shown
/// in the card `slot` names (`pending` or `main`), holding `body`.
pub(crate) fn form(slot: &'static str, class: &'static str, body: impl IntoView) -> impl IntoView {
    view! {
        <form method="post" action=Configure::url() data-slot=slot class=class>
            {body}
        </form>
    }
}

/// A confirmation step: the button `open` names opens the popover `id`,
/// which asks `question`, holds `inside` (a reason, when the change takes
/// one) and runs the change with its `submit` button or closes with Cancel;
/// the browser moves the focus into it as it opens and back to the button
/// as it closes (HTML, the `popover` attribute).
pub(crate) fn confirmed(
    id: String,
    (open, variant): (impl IntoView, Variant),
    (question, submit): (String, &'static str),
    inside: impl IntoView,
) -> impl IntoView {
    let question_id = format!("{id}-question");
    let labelled = question_id.clone();
    let (opens, closes) = (id.clone(), id.clone());
    let confirm = if variant == Variant::Danger {
        Variant::Destructive
    } else {
        Variant::Primary
    };
    let class = if variant == Variant::Danger {
        "fui-dialog fui-dialog-danger"
    } else {
        "fui-dialog"
    };
    view! {
        <button type="button" class=variant.class() popovertarget=opens aria-haspopup="dialog">
            {open}
        </button>
        <div id=id popover="auto" role="dialog" aria-labelledby=labelled class=class>
            <h2 id=question_id class="fui-dialog-title">
                {question}
            </h2>
            <div class="fui-form">{inside}</div>
            <div class="fui-dialog-actions">
                <button
                    type="button"
                    class=Variant::Secondary.class()
                    popovertarget=closes
                    popovertargetaction="hide"
                >
                    {label(Some(Glyph::X), "Cancel")}
                </button>
                <button type="submit" class=confirm.class() autofocus>
                    {submit}
                </button>
            </div>
            {hidden("confirm", String::from("yes"))}
        </div>
    }
}

/// The end of a form that saves a revision: its save button naming the
/// revision it saves as, Cancel back to `back`, and the hint that saving
/// does not apply it.
pub(crate) fn save_bar(loaded: &Loaded<'_>, back: &'static str) -> AnyView {
    let next = format!("Save as {}", loaded.next());
    view! {
        <div class="fui-action-bar">
            <button
                type="submit"
                class=Variant::Primary.class()
                aria-describedby="cfg-save-hint"
            >
                {icon(Glyph::SAVE, "16")}
                {next}
            </button>
            <a href=back class=Variant::Secondary.class()>
                "Cancel"
            </a>
            <p id="cfg-save-hint" class="fui-help">
                "Saving does not apply it: apply it from the card at the top."
            </p>
        </div>
    }
    .into_any()
}

/// The button that adds a row from the template `template` into the list
/// `into`, labelled `label`, with the id `id`, and the line where a refusal
/// of the list shows.
pub(crate) fn add_button(
    id: &str,
    (template, into): (&str, &str),
    (label, placeholder): (&'static str, &'static str),
) -> AnyView {
    let button = view! {
        <button
            type="button"
            id=id.to_owned()
            class=Variant::Secondary.class()
            data-add=template.to_owned()
            data-into=into.to_owned()
            data-placeholder=placeholder
            aria-describedby=error_id(id)
        >
            {icon(Glyph::PLUS, "16")}
            {label}
        </button>
    };
    action_field(button, error_line(id))
}

/// The button that removes the row `row`, named for what the row holds.
pub(crate) fn remove_button(row: &str, named: String) -> AnyView {
    view! {
        <button
            type="button"
            class=Variant::Ghost.class()
            data-remove=row.to_owned()
            aria-label=named
        >
            {icon(Glyph::TRASH, "16")}
            "Remove"
        </button>
    }
    .into_any()
}

/// A template the page's script copies a row from, its key written
/// `placeholder` until a copy replaces it.
pub(crate) fn template(id: String, row: impl IntoView) -> impl IntoView {
    view! { <template id=id>{row}</template> }
}

/// The value `value` of a setting in a diff, or "not set".
fn diff_value(value: Option<&String>, sign: &'static str, removed: bool) -> AnyView {
    match value {
        Some(value) if removed => view! {
            <del class="fui-change fui-change-removed">
                <span aria-hidden="true" class="fui-change-sign">
                    {sign}
                </span>
                <span class="fui-sr-only">"Running value: "</span>
                <code>{value.clone()}</code>
            </del>
        }
        .into_any(),
        Some(value) => view! {
            <ins class="fui-change fui-change-added">
                <span aria-hidden="true" class="fui-change-sign">
                    {sign}
                </span>
                <span class="fui-sr-only">"New value: "</span>
                <code>{value.clone()}</code>
            </ins>
        }
        .into_any(),
        None => view! { <span class="fui-muted">"not set"</span> }.into_any(),
    }
}

/// The settings `changes` names, from the running revision `from` to
/// revision `to`, the running value marked `−` and the new one `+`.
pub(crate) fn diff_table(
    changes: &[Difference],
    (from, to): (Option<RevisionId>, RevisionId),
) -> AnyView {
    if changes.is_empty() {
        return view! { <p class="fui-muted">"It changes no setting."</p> }.into_any();
    }
    let running = from.map_or_else(
        || String::from("Running value"),
        |from| format!("Revision {from} (running)"),
    );
    let (from_label, to_label) = (running.clone(), format!("Revision {to}"));
    let columns = [
        Column::text("setting", "Setting"),
        Column::owned("from", running),
        Column::owned("to", to_label.clone()),
    ];
    let rows = changes
        .iter()
        .map(|change| {
            let what = change.change.as_str();
            view! {
                <tr>
                    <th scope="row">
                        <code>{change.key.clone()}</code>
                        " "
                        {badge(Tone::Neutral, what)}
                    </th>
                    <td data-label=from_label.clone()>{diff_value(change.from.as_ref(), "−", true)}</td>
                    <td data-label=to_label.clone()>{diff_value(change.to.as_ref(), "+", false)}</td>
                </tr>
            }
        })
        .collect_view();
    crate::views::table(
        &format!("What revision {to} changes, by key"),
        &columns,
        rows.into_any(),
    )
}
