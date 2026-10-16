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
use leptos::prelude::*;
use leptos::server_fn::ServerFn as _;

use crate::configure::act::Configure;
use crate::configure::render::Loaded;
use crate::styles;

/// The book page that explains the configuration page.
pub(crate) const LEARN: &str = "https://ferrofed.eu/docs/operate/configuration-revisions.html";

/// The classes of a confirmation popover: a small card in the middle of
/// the window, over a dimmed page.
const POPOVER: &str = "m-auto w-[min(26rem,calc(100vw-2rem))] rounded-card border border-line \
                       bg-raised p-5 text-ink shadow-card backdrop:bg-scrim";

/// The classes of the line under a field that says why it was refused.
const FIELD_ERROR: &str = "m-0 text-xs font-medium text-danger-fg";

/// The classes of a card's one line of explanation.
const LEDE: &str = "mt-0 mb-4 text-sm text-muted";

/// One line of explanation and a link to the book's section `anchor`.
pub(crate) fn lede(text: &'static str, anchor: &'static str) -> AnyView {
    view! {
        <p class=LEDE>
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
        <div class="flex flex-wrap items-start justify-between gap-3">
            <div class="min-w-0">
                <h2 id=id class="mt-0 mb-1 text-base font-semibold text-ink">
                    {title}
                </h2>
                {lede(line, anchor)}
            </div>
            <div class="flex flex-wrap items-center gap-2">{actions}</div>
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
    view! { <p id=format!("{id}-error") class=FIELD_ERROR data-field-error="" hidden></p> }
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
    let hint_id = format!("{id}-hint");
    let described = if shown.hint.is_some() {
        format!("{hint_id} {id}-error")
    } else {
        format!("{id}-error")
    };
    let kind = if shown.kind.is_empty() {
        "text"
    } else {
        shown.kind
    };
    let class = if shown.mono { styles::MONO } else { "" };
    view! {
        <div class=styles::FIELD>
            <label for=id.to_owned()>{label}</label>
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
            {shown
                .hint
                .map(|hint| {
                    view! {
                        <p id=hint_id class=styles::HELP>
                            {hint}
                        </p>
                    }
                })}
            {error_line(id)}
        </div>
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
    (open, variant): (impl IntoView, styles::Button),
    (question, submit): (String, &'static str),
    inside: impl IntoView,
) -> impl IntoView {
    let question_id = format!("{id}-question");
    let labelled = question_id.clone();
    let (opens, closes) = (id.clone(), id.clone());
    view! {
        <button
            type="button"
            class=styles::button(variant)
            popovertarget=opens
            aria-haspopup="dialog"
        >
            {open}
        </button>
        <div id=id popover="auto" role="dialog" aria-labelledby=labelled class=POPOVER>
            <p id=question_id class="mt-0 mb-3 font-medium">
                {question}
            </p>
            {inside}
            <div class="mt-4 flex flex-wrap justify-end gap-2">
                <button
                    type="button"
                    class=styles::BUTTON_SECONDARY
                    popovertarget=closes
                    popovertargetaction="hide"
                >
                    "Cancel"
                </button>
                <button type="submit" class=styles::BUTTON_PRIMARY autofocus>
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
        <div class="flex flex-wrap items-center gap-3 border-t border-line pt-4">
            <button type="submit" class=styles::BUTTON_PRIMARY aria-describedby="cfg-save-hint">
                {next}
            </button>
            <a href=back class=styles::BUTTON_SECONDARY>
                "Cancel"
            </a>
            <p id="cfg-save-hint" class="m-0 text-sm text-muted">
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
    view! {
        <div class="flex flex-col gap-1">
            <button
                type="button"
                id=id.to_owned()
                class=format!("{} self-start", styles::BUTTON_SECONDARY)
                data-add=template.to_owned()
                data-into=into.to_owned()
                data-placeholder=placeholder
                aria-describedby=format!("{id}-error")
            >
                {crate::components::icon(icondata_lu::LuPlus, "16")}
                {label}
            </button>
            {error_line(id)}
        </div>
    }
    .into_any()
}

/// The button that removes the row `row`, named for what the row holds.
pub(crate) fn remove_button(row: &str, named: String) -> AnyView {
    view! {
        <button
            type="button"
            class=format!("{} self-start", styles::BUTTON_GHOST)
            data-remove=row.to_owned()
            aria-label=named
        >
            {crate::components::icon(icondata_lu::LuTrash2, "16")}
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
            <del class="no-underline">
                <span aria-hidden="true" class="mr-1 font-semibold text-danger-fg">
                    {sign}
                </span>
                <span class="sr-only">"Running value: "</span>
                <code>{value.clone()}</code>
            </del>
        }
        .into_any(),
        Some(value) => view! {
            <ins class="no-underline">
                <span aria-hidden="true" class="mr-1 font-semibold text-ok-fg">
                    {sign}
                </span>
                <span class="sr-only">"New value: "</span>
                <code>{value.clone()}</code>
            </ins>
        }
        .into_any(),
        None => view! { <span class="text-muted">"not set"</span> }.into_any(),
    }
}

/// The settings `changes` names, from the running revision `from` to
/// revision `to`, the running value marked `−` and the new one `+`.
pub(crate) fn diff_table(
    changes: &[Difference],
    (from, to): (Option<RevisionId>, RevisionId),
) -> AnyView {
    if changes.is_empty() {
        return view! { <p class="text-muted">"It changes no setting."</p> }.into_any();
    }
    let rows = changes
        .iter()
        .map(|change| {
            let what = change.change.as_str();
            view! {
                <tr>
                    <th scope="row">
                        <code>{change.key.clone()}</code>
                        <span class="ml-2 badge tone-neutral">{what}</span>
                    </th>
                    <td>{diff_value(change.from.as_ref(), "−", true)}</td>
                    <td>{diff_value(change.to.as_ref(), "+", false)}</td>
                </tr>
            }
        })
        .collect_view();
    let running = from.map_or_else(
        || String::from("Running value"),
        |from| format!("Revision {from} (running)"),
    );
    view! {
        <div
            class=format!("{} relative", styles::TABLE_SCROLL)
            tabindex="0"
            role="region"
            aria-label=format!("What revision {to} changes")
        >
            <table>
                <caption class="sr-only">{format!("What revision {to} changes, by key")}</caption>
                <thead>
                    <tr>
                        <th scope="col">"Setting"</th>
                        <th scope="col">{running}</th>
                        <th scope="col">{format!("Revision {to}")}</th>
                    </tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
        </div>
    }
    .into_any()
}
