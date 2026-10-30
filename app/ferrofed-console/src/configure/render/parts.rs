// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The pieces every part of the configuration page is drawn from: a card's
//! head with its one line and its "Learn more" link, a field with the place
//! its refusal is shown, a confirmation step in a popover, a repeatable row
//! and the button that adds one, and the table of what a revision changes.
//!
//! Every form posts to the configuration page it sits on, which the server
//! draws again with the answer. The repeatable rows are FerroUI's F7 rows:
//! an Add or a Remove submits its form with `form[edit]` naming the row,
//! and the page answers with the form drawn again from what was posted
//! (`crate::configure::draft`), so the rows change with the scripts
//! refused; with them, FerroUI's `repeat` module changes them in place and
//! the form posts the rows it holds. A refused field's line shows the
//! refusal the page was drawn with ([`drawing`]).

use std::borrow::Cow;

use ferrofed_registry::revision::{Change, Difference, RevisionId};
use ferrohealth_ui::html::Html;
use ferrohealth_ui::icon::{Glyph, icon};
use ferrohealth_ui::table::view::Column;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action_menu::{Act, Action, Item, Menu, row_menu};
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::dialog::{ConfirmForm, Confirmation, danger_form_dialog};
use ferrohealth_ui::view::form::{ERROR, Field, INPUT, described_by, error_id};
use ferrohealth_ui::view::repeat::{RepeatAdd, RowEnd};
use ferrohealth_ui::view::status::badge;

use crate::configure::draft::{Draft, EDIT, Edit};
use crate::configure::plan::removal_reason;
use crate::configure::render::Loaded;
use crate::configure::said::FieldNote;
use crate::locale::{Msg, current, tr, tr_with};
use crate::pages::{h, t};

/// The book page that explains the configuration page.
pub(crate) const LEARN: &str = "https://ferrofed.eu/docs/operate/configuration-revisions.html";

tokio::task_local! {
    /// The form the page draws again, as posted, with the field a refusal
    /// names.
    static POSTED: Option<Draft>;
}

/// Runs `draw` with `posted` as the form the page draws again, so each
/// field shows what was posted and a refused field its refusal.
pub(crate) fn drawing<R>(posted: Option<Draft>, draw: impl FnOnce() -> R) -> R {
    POSTED.sync_scope(posted, draw)
}

/// The form the page draws again, if a post drew it.
pub(crate) fn posted() -> Option<Draft> {
    POSTED.try_with(Clone::clone).ok().flatten()
}

/// One line of explanation and a link to the book's section `anchor`.
pub(crate) fn lede(text: String, anchor: &'static str) -> Html {
    h(format!(
        "<p class=\"fui-muted\">{} <a href=\"{LEARN}#{anchor}\" rel=\"external\">{}</a></p>",
        t(text),
        t(tr(Msg::LearnMore))
    ))
}

/// A card's head: its heading `title` with the id `id`, the one line
/// `line` explaining it with the book's `anchor`, and `actions` at its end.
pub(crate) fn head(
    (id, title): (&'static str, String),
    (line, anchor): (String, &'static str),
    actions: Html,
) -> Html {
    let actions = String::from(actions);
    h(format!(
        "<div class=\"fui-split\"><div><h2 id=\"{id}\" class=\"fui-card-title\">{}</h2>{}</div>\
         <div class=\"fui-cluster\">{actions}</div></div>",
        t(title),
        lede(line, anchor)
    ))
}

/// A hidden field `name` of a form, holding `value`.
pub(crate) fn hidden(name: &str, value: String) -> Html {
    h(format!(
        "<input type=\"hidden\" name=\"form[{}]\" value=\"{}\">",
        t(name),
        t(value)
    ))
}

/// The refusal of the field `id`, when the page answers one of it.
fn refusal(id: &str) -> Option<String> {
    posted()
        .and_then(|draft| draft.field)
        .filter(|note: &FieldNote| note.id == id)
        .map(|note| note.message)
}

/// The line under the field `id` where its refusal shows, hidden while the
/// page answers none of it.
pub(crate) fn error_line(id: &str) -> Html {
    let said = refusal(id);
    h(format!(
        "<p id=\"{}\" class=\"{ERROR}\"{}>{}</p>",
        t(error_id(id)),
        if said.is_none() { " hidden" } else { "" },
        t(said.unwrap_or_default())
    ))
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
pub(crate) fn text((id, name): (&str, &str), label: String, value: String, shown: Text) -> Html {
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
    let mut attributes = String::new();
    if let Some(list) = shown.list {
        attributes.push_str(" list=\"");
        attributes.push_str(list);
        attributes.push('"');
    }
    if shown.required {
        attributes.push_str(" required");
    }
    if shown.readonly {
        attributes.push_str(" readonly");
    }
    if refusal(id).is_some() {
        attributes.push_str(" aria-invalid=\"true\"");
    }
    if let Some(described) = described {
        attributes.push_str(" aria-describedby=\"");
        attributes.push_str(t(described).as_str());
        attributes.push('"');
    }
    let input = h(format!(
        "<input type=\"{kind}\" id=\"{}\" name=\"{}\" value=\"{}\" class=\"{class}\" \
         autocomplete=\"off\" spellcheck=\"false\"{attributes}>",
        t(id),
        t(name),
        t(value)
    ));
    Field::new(id, label)
        .help_if(shown.hint)
        .after(error_line(id))
        .view(input)
}

/// The breadcrumb trail from the tab `(label, href)` to the form or view
/// titled `current`, the current one last (WAI-ARIA Authoring Practices, the
/// breadcrumb pattern).
pub(crate) fn trail((label, href): (String, &'static str), current: String) -> Html {
    h(format!(
        "<nav class=\"fui-crumbs\" aria-label=\"{}\"><ol><li><a href=\"{href}\">{}</a>\
         <span class=\"fui-crumb-sep\" aria-hidden=\"true\">/</span></li>\
         <li aria-current=\"page\">{}</li></ol></nav>",
        t(tr(Msg::Breadcrumb)),
        t(label),
        t(current)
    ))
}

/// The reason field every change ends with, holding the reason the
/// operator posted when the page draws their form again.
pub(crate) fn reason() -> Html {
    let posted = posted().map(|draft| draft.form.reason).unwrap_or_default();
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
}

/// The address a configuration form posts to: the page it sits on, with
/// the query that names it, which the server draws again with the answer.
pub(crate) fn action() -> String {
    crate::pages::here_with_query()
}

/// A form posting to the configuration page it sits on ([`action`]),
/// holding `body`, with `class`.
///
/// Its first submit button posts the form as it stands and is hidden from
/// sight and from a reader, so Enter in a field saves rather than pressing
/// the first Add or Remove (HTML, "implicit submission").
pub(crate) fn form(class: &'static str, body: Html) -> Html {
    let body = String::from(body);
    h(format!(
        "<form method=\"post\" action=\"{}\" class=\"{class}\">\
         <button type=\"submit\" class=\"fui-sr-only\" tabindex=\"-1\" aria-hidden=\"true\"></button>\
         {body}</form>",
        t(action())
    ))
}

/// What a configuration table row's Remove… asks and posts.
pub(crate) struct Removal {
    /// The question, such as "Remove member node-b?".
    pub(crate) title: String,
    /// The sentence that says what saving the removal does.
    pub(crate) message: String,
    /// The hidden fields naming what it removes, and its required Reason;
    /// the dialog adds the field that says the step was confirmed.
    pub(crate) fields: Html,
}

/// The actions of a configuration table's row named `named`, whose
/// removal confirms in the dialog `dialog`: its row menu (F13) with Edit to
/// `edit` and, when the row can be removed, Remove… after a separator,
/// opening that dialog (FerroUI `danger_form_dialog`) with its Reason,
/// drawn beside the menu in the row's actions cell. The dialog's form posts
/// to the page it sits on.
pub(crate) fn row_actions(
    dialog: &str,
    named: &str,
    edit: String,
    removal: Option<Removal>,
) -> Html {
    let locale = current();
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
        return row_menu(&locale, &menu);
    };
    let menu = menu.destructive(vec![Item::new(Action::new(
        Some(Glyph::TRASH),
        tr(Msg::Remove),
        Act::Dialog(dialog.to_owned()),
    ))]);
    let confirm = h(format!(
        "<button type=\"submit\" class=\"{}\">{}</button>",
        Variant::Destructive.class(),
        ferrohealth_ui::view::button::label(Some(Glyph::TRASH), tr(Msg::SaveTheRemoval))
    ));
    let fields = removal
        .fields
        .then(hidden("confirm", String::from("yes")));
    let asks = danger_form_dialog(
        &locale,
        Confirmation {
            id: dialog.to_owned().into(),
            title: removal.title.into(),
            message: removal.message.into(),
        },
        ConfirmForm {
            action: action(),
            fields,
        },
        confirm,
    );
    row_menu(&locale, &menu).then(asks)
}

/// The required Reason of the removal of `named`, empty as it opens.
pub(crate) fn removal_reason_field(named: &str) -> Html {
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
/// submit posts that form, with the scripts refused too.
pub(crate) fn confirmed(
    id: impl Into<Cow<'static, str>>,
    (glyph, words, variant): (Glyph, String, Variant),
    (question, submit): (String, String),
    inside: Html,
) -> Html {
    let id: Cow<'static, str> = id.into();
    let confirm = if variant == Variant::Danger {
        Variant::Destructive
    } else {
        Variant::Primary
    };
    let inside = String::from(inside);
    let body = h(format!(
        "<div class=\"fui-form\">{inside}</div><div class=\"fui-dialog-actions\">\
         <button type=\"button\" class=\"{}\" command=\"close\" commandfor=\"{}\">{}</button>\
         <button type=\"submit\" class=\"{}\">{}</button></div>{}",
        Variant::Secondary.class(),
        t(&*id),
        ferrohealth_ui::view::button::label(Some(Glyph::X), tr(Msg::Cancel)),
        confirm.class(),
        t(submit),
        hidden("confirm", String::from("yes")),
    ));
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
pub(crate) fn save_bar(loaded: &Loaded<'_>, back: &'static str) -> Html {
    let next = tr_with(Msg::SaveAs, &[("loaded", (loaded.next()).clone().into())]);
    h(format!(
        "<div class=\"fui-actions\">\
         <button type=\"submit\" class=\"{}\" aria-describedby=\"cfg-save-hint\">{}{}</button>\
         <a href=\"{back}\" class=\"{}\">{}</a>\
         <p id=\"cfg-save-hint\" class=\"fui-help\">{}</p></div>",
        Variant::Primary.class(),
        icon(Glyph::SAVE, "16"),
        t(next),
        Variant::Secondary.class(),
        t(tr(Msg::Cancel)),
        t(tr(Msg::SavingDoesNotApplyApplyFrom)),
    ))
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
fn diff_value(value: Option<&String>, sign: &'static str, removed: bool) -> Html {
    match value {
        Some(value) if removed => h(format!(
            "<del class=\"fui-change fui-change-removed\"><span aria-hidden=\"true\" class=\"fui-change-sign\">{sign}</span>\
             <span class=\"fui-sr-only\">{}</span><code>{}</code></del>",
            t(tr(Msg::RunningValue2)),
            t(value)
        )),
        Some(value) => h(format!(
            "<ins class=\"fui-change fui-change-added\"><span aria-hidden=\"true\" class=\"fui-change-sign\">{sign}</span>\
             <span class=\"fui-sr-only\">{}</span><code>{}</code></ins>",
            t(tr(Msg::NewValue)),
            t(value)
        )),
        None => h(format!(
            "<span class=\"fui-muted\">{}</span>",
            t(tr(Msg::NotSet2))
        )),
    }
}

/// The settings `changes` names, from the running revision `from` to
/// revision `to`, the running value marked `−` and the new one `+`.
pub(crate) fn diff_table(
    changes: &[Difference],
    (from, to): (Option<RevisionId>, RevisionId),
) -> Html {
    if changes.is_empty() {
        return h(format!(
            "<p class=\"fui-muted\">{}</p>",
            t(tr(Msg::ItChangesNoSetting))
        ));
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
    let rows = changes.iter().map(|change| {
        let what = tr(match change.change {
            Change::Added => Msg::DiffAdded,
            Change::Removed => Msg::DiffRemoved,
            _ => Msg::DiffChanged,
        });
        let mark = badge(Tone::Neutral, &what);
        h(format!(
            "<tr><th scope=\"row\"><code>{}</code> {mark}</th>\
             <td data-label=\"{}\">{}</td><td data-label=\"{}\">{}</td></tr>",
            t(&change.key),
            t(&from_label),
            diff_value(change.from.as_ref(), "\u{2212}", true),
            t(&to_label),
            diff_value(change.to.as_ref(), "+", false),
        ))
    });
    crate::pages::table(
        &tr_with(
            Msg::WhatRevisionChangesKey,
            &[("to", (to).to_string().into())],
        ),
        &columns,
        Html::join(rows),
    )
}
