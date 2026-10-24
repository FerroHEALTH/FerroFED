// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The pieces every part of the configuration page is drawn from: a card's
//! head with its one line and its "Learn more" link, a field with the place
//! its refusal is shown, a confirmation step in a popover, a repeatable row
//! and the button that adds one, and the table of what a revision changes.
//!
//! The repeatable rows are FerroUI's F7 rows: an Add or a Remove submits
//! its form with `form[edit]` naming the row, and the page answers with the
//! form drawn again from what was posted (`crate::configure::draft`), so
//! the rows change with no WebAssembly; with it, FerroUI's island changes
//! them in place and the form posts the rows it holds. A refused field's line shows the refusal the page was
//! drawn with.

use std::borrow::Cow;

use ferrofed_registry::revision::{Difference, RevisionId};
use ferrohealth_ui::icon::{Glyph, icon};
use ferrohealth_ui::table::view::Column;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::dialog::{ConfirmForm, Confirmation, danger_form_dialog};
use ferrohealth_ui::view::form::{ERROR, Field, INPUT, described_by, error_id};
use ferrohealth_ui::view::status::badge;
use ferrohealth_ui::widget::action_menu::{Act, Action, Item, Menu, row_menu};
use ferrohealth_ui::widget::repeat::{RepeatAdd, RowEnd};
use leptos::prelude::*;

use crate::configure::draft::{Draft, EDIT, Edit};
use crate::configure::plan::removal_reason;
use crate::configure::render::Loaded;
use crate::configure::said::FieldNote;
use crate::locale::{Msg, tr, tr_with};

/// The book page that explains the configuration page.
pub(crate) const LEARN: &str = "https://ferrofed.eu/docs/operate/configuration-revisions.html";

/// One line of explanation and a link to the book's section `anchor`.
pub(crate) fn lede(text: String, anchor: &'static str) -> AnyView {
    view! {
        <p class="fui-muted">
            {text} " " <a href=format!("{LEARN}#{anchor}") rel="external">
                {tr(Msg::LearnMore)}
            </a>
        </p>
    }
    .into_any()
}

/// A card's head: its heading `title` with the id `id`, the one line
/// `line` explaining it with the book's `anchor`, and `actions` at its end.
pub(crate) fn head(
    (id, title): (&'static str, String),
    (line, anchor): (String, &'static str),
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

/// The refusal of the field `id`, when the page answers one of it.
fn refusal(id: &str) -> Option<String> {
    use_context::<FieldNote>()
        .filter(|note| note.id == id)
        .map(|note| note.message)
}

/// The line under the field `id` where its refusal shows, hidden while the
/// page answers none of it.
pub(crate) fn error_line(id: &str) -> impl IntoView + use<> {
    let said = refusal(id);
    let hidden = said.is_none();
    view! {
        <p id=error_id(id) class=ERROR hidden=hidden>
            {said}
        </p>
    }
}

/// What a text field shows and takes beside its value.
#[derive(Debug, Clone, Default)]
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
    pub(crate) hint: Option<String>,
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
            aria-invalid=refusal(id).map(|_| "true")
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
pub(crate) fn trail((label, href): (String, &'static str), current: String) -> AnyView {
    view! {
        <nav class="fui-crumbs" aria-label=tr(Msg::Breadcrumb)>
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

/// The reason field every change ends with, holding the reason the
/// operator posted when the page draws their form again.
pub(crate) fn reason() -> AnyView {
    let posted = use_context::<Draft>()
        .map(|draft| draft.form.reason)
        .unwrap_or_default();
    text(
        (crate::configure::plan::REASON, "form[reason]"),
        tr(Msg::Reason),
        posted,
        Text {
            required: true,
            hint: Some(tr(Msg::HistoryRecordsRevision)),
            ..Text::default()
        },
    )
    .into_any()
}

/// A form posting to the page it sits on, holding `body`, with `class`.
///
/// Its first submit button posts the form as it stands and is hidden from
/// sight and from a reader, so Enter in a field saves rather than pressing
/// the first Add or Remove (HTML, "implicit submission").
pub(crate) fn form(class: &'static str, body: impl IntoView) -> impl IntoView {
    view! {
        <form method="post" action=crate::pages::here() class=class>
            <button type="submit" class="fui-sr-only" tabindex="-1" aria-hidden="true"></button>
            {body}
        </form>
    }
}

/// What a configuration table row's Remove… asks and posts.
pub(crate) struct Removal {
    /// The question, such as "Remove member node-b?".
    pub(crate) title: String,
    /// The sentence that says what saving the removal does.
    pub(crate) message: String,
    /// The hidden fields naming what it removes, and its required Reason;
    /// the dialog adds the field that says the step was confirmed.
    pub(crate) fields: AnyView,
}

/// The actions of a configuration table's row named `named`, whose
/// removal confirms in the dialog `dialog`: its row menu (F13) with Edit to
/// `edit` and, when the row can be removed, Remove… after a separator,
/// opening that dialog (FerroUI `danger_form_dialog`) with its Reason,
/// drawn beside the menu in the row's actions cell.
pub(crate) fn row_actions(
    dialog: &str,
    named: &str,
    edit: String,
    removal: Option<Removal>,
) -> AnyView {
    let menu = Menu::new(
        format!("{}-menu", dialog.trim_end_matches("-dialog")),
        tr_with(Msg::ActionsFor, &[("named", named.to_owned().into())]),
        vec![Item::new(Action::new(
            Some(Glyph::PENCIL),
            tr(Msg::Edit),
            Act::Link(edit),
        ))],
    );
    let Some(removal) = removal else {
        return row_menu(menu);
    };
    let menu = menu.destructive(vec![Item::new(Action::new(
        Some(Glyph::TRASH),
        tr(Msg::Remove),
        Act::Dialog(dialog.to_owned()),
    ))]);
    let confirm = view! {
        <button type="submit" class=Variant::Destructive.class()>
            {ferrohealth_ui::view::button::label(Some(Glyph::TRASH), tr(Msg::SaveTheRemoval))}
        </button>
    };
    let asks = danger_form_dialog(
        Confirmation {
            id: dialog.to_owned().into(),
            title: removal.title.into(),
            message: removal.message.into(),
        },
        ConfirmForm {
            action: crate::pages::here(),
            fields: view! {
                {removal.fields}
                {hidden("confirm", String::from("yes"))}
            }
            .into_any(),
        },
        confirm,
    );
    view! {
        {row_menu(menu)}
        {asks}
    }
    .into_any()
}

/// The required Reason of the removal of `named`, empty as it opens.
pub(crate) fn removal_reason_field(named: &str) -> AnyView {
    text(
        (&removal_reason(named), "form[reason]"),
        tr(Msg::Reason),
        String::new(),
        Text {
            required: true,
            ..Text::default()
        },
    )
}

/// A confirmation step (FerroUI R17 anchored): the button `label` opens the
/// modal dialog `id`, which asks `question`, holds `inside` (a reason, when
/// the change takes one) and runs the change with its `submit` button or
/// closes with Cancel. The dialog sits inside the form it confirms, so its
/// submit posts that form, with no WebAssembly.
pub(crate) fn confirmed(
    id: impl Into<Cow<'static, str>>,
    (glyph, words, variant): (Glyph, String, Variant),
    (question, submit): (String, String),
    inside: impl IntoView + 'static,
) -> AnyView {
    let id: Cow<'static, str> = id.into();
    let confirm = if variant == Variant::Danger {
        Variant::Destructive
    } else {
        Variant::Primary
    };
    let body = view! {
        <div class="fui-form">{inside}</div>
        <div class="fui-dialog-actions">
            <button
                type="button"
                class=Variant::Secondary.class()
                command="close"
                commandfor=id.to_string()
            >
                {ferrohealth_ui::view::button::label(Some(Glyph::X), tr(Msg::Cancel))}
            </button>
            <button type="submit" class=confirm.class()>
                {submit}
            </button>
        </div>
        {hidden("confirm", String::from("yes"))}
    }
    .into_any();
    ferrohealth_ui::view::popover::popover_form(ferrohealth_ui::view::popover::PopoverForm {
        id,
        variant,
        glyph,
        label: words.into(),
        title: question.into(),
        body,
    })
}

/// The end of a form that saves a revision: its save button naming the
/// revision it saves as, Cancel back to `back`, and the hint that saving
/// does not apply it.
pub(crate) fn save_bar(loaded: &Loaded<'_>, back: &'static str) -> AnyView {
    let next = tr_with(Msg::SaveAs, &[("loaded", (loaded.next()).clone().into())]);
    view! {
        <div class="fui-actions">
            <button type="submit" class=Variant::Primary.class() aria-describedby="cfg-save-hint">
                {icon(Glyph::SAVE, "16")}
                {next}
            </button>
            <a href=back class=Variant::Secondary.class()>
                {tr(Msg::Cancel)}
            </a>
            <p id="cfg-save-hint" class="fui-help">
                {tr(Msg::SavingDoesNotApplyApplyFrom)}
            </p>
        </div>
    }
    .into_any()
}

/// The Add under a list of F7 rows, labelled `label`, which posts the
/// form with `edit`.
pub(crate) fn row_add(edit: &Edit, label: String) -> RepeatAdd {
    RepeatAdd {
        label,
        name: String::from(EDIT),
        value: edit.value(),
    }
}

/// The Remove that ends an F7 row, named `named` for what the row holds,
/// which posts the form with `edit`.
pub(crate) fn row_remove(edit: &Edit, named: String) -> RowEnd {
    RowEnd::Remove {
        label: named,
        name: String::from(EDIT),
        value: edit.value(),
    }
}

/// The value `value` of a setting in a diff, or "not set".
fn diff_value(value: Option<&String>, sign: &'static str, removed: bool) -> AnyView {
    match value {
        Some(value) if removed => view! {
            <del class="fui-change fui-change-removed">
                <span aria-hidden="true" class="fui-change-sign">
                    {sign}
                </span>
                <span class="fui-sr-only">{tr(Msg::RunningValue2)}</span>
                <code>{value.clone()}</code>
            </del>
        }
        .into_any(),
        Some(value) => view! {
            <ins class="fui-change fui-change-added">
                <span aria-hidden="true" class="fui-change-sign">
                    {sign}
                </span>
                <span class="fui-sr-only">{tr(Msg::NewValue)}</span>
                <code>{value.clone()}</code>
            </ins>
        }
        .into_any(),
        None => view! { <span class="fui-muted">{tr(Msg::NotSet2)}</span> }.into_any(),
    }
}

/// The settings `changes` names, from the running revision `from` to
/// revision `to`, the running value marked `−` and the new one `+`.
pub(crate) fn diff_table(
    changes: &[Difference],
    (from, to): (Option<RevisionId>, RevisionId),
) -> AnyView {
    if changes.is_empty() {
        return view! { <p class="fui-muted">{tr(Msg::ItChangesNoSetting)}</p> }.into_any();
    }
    let running = from.map_or_else(
        || tr(Msg::RunningValue),
        |from| tr_with(Msg::RevisionRunning, &[("from", (from).to_string().into())]),
    );
    let (from_label, to_label) = (
        running.clone(),
        tr_with(Msg::Revision5, &[("to", (to).to_string().into())]),
    );
    let columns = [
        Column::text("setting", tr(Msg::Setting)),
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
                    <td data-label=from_label
                        .clone()>{diff_value(change.from.as_ref(), "−", true)}</td>
                    <td data-label=to_label
                        .clone()>{diff_value(change.to.as_ref(), "+", false)}</td>
                </tr>
            }
        })
        .collect_view();
    crate::pages::table(
        &tr_with(
            Msg::WhatRevisionChangesKey,
            &[("to", (to).to_string().into())],
        ),
        &columns,
        rows.into_any(),
    )
}
