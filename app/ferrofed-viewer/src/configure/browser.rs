// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the browser half adds to the configuration page, whose markup the
//! server rendered: the form submitted, read into its typed fields; a
//! repeatable row added from its template or removed; the tabs moved
//! between with the arrow keys (WAI-ARIA Authoring Practices, the tabs
//! pattern); an import's lines counted; a refused field marked and the
//! focus moved to the answer (WCAG 2.2 SC 2.4.3, 3.3.1); and the countdown
//! of a confirmation. On the server half each does nothing. No
//! specification governs this: our own design.

#[cfg(target_arch = "wasm32")]
use leptos::prelude::*;

use crate::admin::model::FieldNote;
use crate::configure::model::ConfigForm;

/// The form `event` submits, read into its typed fields, and whether it
/// sits in the card of the saved revision; `None` when the browser gives no
/// form data, which leaves the plain post the server refuses. The
/// confirmation step it was sent from closes.
pub(super) fn submitted(event: &leptos::ev::SubmitEvent) -> Option<(ConfigForm, bool)> {
    #[cfg(target_arch = "wasm32")]
    {
        use leptos::web_sys::{FormData, HtmlElement, HtmlFormElement};
        use wasm_bindgen::JsCast as _;
        let form = event.target()?.dyn_into::<HtmlFormElement>().ok()?;
        let data = FormData::new_with_form(&form).ok()?;
        let pending = form.get_attribute("data-slot").as_deref() == Some("pending");
        let open = form
            .query_selector(":popover-open")
            .ok()
            .flatten()
            .and_then(|open| open.dyn_into::<HtmlElement>().ok());
        if let Some(open) = open
            && open.hide_popover().is_err()
        {
            leptos::logging::warn!("the browser refused to close the confirmation");
        }
        Some((read::form(&data), pending))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _: &leptos::ev::SubmitEvent = event;
        None
    }
}

/// Reading a form's typed fields from the browser's form data.
#[cfg(target_arch = "wasm32")]
mod read {
    use leptos::web_sys::FormData;

    use crate::configure::model::{
        ConfigForm, FhirEndpointInput, QueryEndpointInput, SecretInput, SettingInput, Typed,
    };

    /// The value of the field `name`, empty when the form sends none.
    fn one(data: &FormData, name: &str) -> String {
        // NOTE: XHR standard, `FormData.get`: a field the form does not send, a
        // clear checkbox among them, is null, which is the empty field it means.
        data.get(name).as_string().unwrap_or_default()
    }

    /// Every value of the fields named `name`, in the order of the form.
    fn all(data: &FormData, name: &str) -> Vec<String> {
        data.get_all(name)
            .iter()
            .filter_map(|value| value.as_string())
            .collect()
    }

    /// The configuration form `data` holds, by the field names the forms
    /// post: each row's fields named by its key.
    pub(super) fn form(data: &FormData) -> ConfigForm {
        let field = |name: &str| one(data, &format!("form[{name}]"));
        let query_endpoints = all(data, "form[query_rows]")
            .into_iter()
            .map(|key| {
                let part = |part: &str| one(data, &format!("form[query][{key}][{part}]"));
                QueryEndpointInput {
                    id: part("id"),
                    url: part("url"),
                    in_service: !part("in_service").is_empty(),
                    key,
                }
            })
            .collect();
        let fhir_endpoints = all(data, "form[fhir_rows]")
            .into_iter()
            .map(|key| {
                let part = |part: &str| one(data, &format!("form[fhir][{key}][{part}]"));
                let many = |part: &str| all(data, &format!("form[fhir][{key}][{part}]"));
                FhirEndpointInput {
                    id: part("id"),
                    url: part("url"),
                    system: part("system"),
                    categories: many("categories"),
                    profiles: many("profiles"),
                    in_service: !part("in_service").is_empty(),
                    key,
                }
            })
            .collect();
        let settings = all(data, "form[setting_rows]")
            .into_iter()
            .map(|row| {
                let part = |part: &str| format!("form[setting][{row}][{part}]");
                SettingInput {
                    key: one(data, &part("key")),
                    value: one(data, &part("value")),
                    values: all(data, &part("values")),
                    row,
                }
            })
            .collect();
        let secrets = all(data, "form[secret_rows]")
            .into_iter()
            .map(|row| {
                let part = |part: &str| one(data, &format!("form[secret][{row}][{part}]"));
                SecretInput {
                    key: part("key"),
                    choice: part("choice"),
                    value: Typed::new(part("value")),
                    file: part("file"),
                    row,
                }
            })
            .collect();
        ConfigForm {
            action: field("action"),
            base: field("base"),
            revision: field("revision"),
            current: field("current"),
            reason: field("reason"),
            confirm: field("confirm"),
            organisation: field("organisation"),
            organisation_name: field("organisation_name"),
            node: field("node"),
            system_id: field("system_id"),
            product: field("product"),
            product_version: field("product_version"),
            query_endpoints,
            fhir_endpoints,
            fhir_shown: field("fhir_shown"),
            name: field("name"),
            version: field("version"),
            aql: field("aql"),
            endpoint: field("endpoint"),
            section: field("section"),
            resource: field("resource"),
            grant: field("grant"),
            settings,
            secrets,
            toml: field("toml"),
        }
    }
}

/// The element under `event`, when it is one.
#[cfg(target_arch = "wasm32")]
fn target(event: &leptos::web_sys::Event) -> Option<leptos::web_sys::Element> {
    use wasm_bindgen::JsCast as _;
    event.target()?.dyn_into::<leptos::web_sys::Element>().ok()
}

/// Moves the focus to `element`, when it takes the focus.
#[cfg(target_arch = "wasm32")]
fn focused(element: Option<leptos::web_sys::Element>) {
    use wasm_bindgen::JsCast as _;
    let element =
        element.and_then(|element| element.dyn_into::<leptos::web_sys::HtmlElement>().ok());
    if let Some(element) = element
        && element.focus().is_err()
    {
        leptos::logging::warn!("the browser refused the focus");
    }
}

/// The key the next row added takes, unlike any key the server wrote.
#[cfg(target_arch = "wasm32")]
fn next_key() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    /// How many rows the page has added.
    static ADDED: AtomicU32 = AtomicU32::new(0);
    let added = ADDED.fetch_add(1, Ordering::Relaxed);
    format!("n{added}")
}

/// Adds a row when `event` presses a button that adds one, and removes one
/// when it presses a row's remove button.
pub(super) fn clicked(event: &leptos::ev::MouseEvent) {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(target) = target(event) else {
            return;
        };
        if let Some(button) = target.closest("[data-add]").ok().flatten() {
            added(&button);
        } else if let Some(button) = target.closest("[data-remove]").ok().flatten() {
            removed(&button);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _: &leptos::ev::MouseEvent = event;
}

/// Adds a row from the template `button` names to the list it names, its
/// key a new one, and moves the focus to its first field.
#[cfg(target_arch = "wasm32")]
fn added(button: &leptos::web_sys::Element) {
    let document = document();
    let named = |name: &str| {
        button
            .get_attribute(name)
            .and_then(|id| document.get_element_by_id(&id))
    };
    let (Some(template), Some(into)) = (named("data-add"), named("data-into")) else {
        return;
    };
    let placeholder = button
        .get_attribute("data-placeholder")
        .unwrap_or_else(|| String::from("__KEY__"));
    let row = template.inner_html().replace(&placeholder, &next_key());
    if into.insert_adjacent_html("beforeend", &row).is_err() {
        leptos::logging::warn!("the browser refused the new row");
        return;
    }
    let first = into.last_element_child().and_then(|row| {
        row.query_selector("input:not([type=hidden]), select")
            .ok()
            .flatten()
    });
    focused(first);
}

/// Removes the row `button` names, and moves the focus to the button that
/// adds a row to its list.
#[cfg(target_arch = "wasm32")]
fn removed(button: &leptos::web_sys::Element) {
    let document = document();
    let Some(row) = button
        .get_attribute("data-remove")
        .and_then(|id| document.get_element_by_id(&id))
    else {
        return;
    };
    let list = row.parent_element().map(|list| list.id());
    row.remove();
    let adds = list.and_then(|list| {
        document
            .query_selector(&format!("[data-into=\"{list}\"]"))
            .ok()
            .flatten()
    });
    focused(adds);
}

/// Moves between the tabs when `event` presses an arrow key, Home or End
/// on one; Enter opens the tab, as on any link.
pub(super) fn keyed(event: &leptos::ev::KeyboardEvent) {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(tab) =
            target(event).filter(|tab| tab.get_attribute("role").as_deref() == Some("tab"))
        else {
            return;
        };
        let mut tabs = Vec::new();
        let mut next = tab
            .closest("[role=tablist]")
            .ok()
            .flatten()
            .and_then(|list| list.first_element_child());
        while let Some(one) = next {
            next = one.next_element_sibling();
            tabs.push(one);
        }
        let Some(at) = tabs.iter().position(|one| *one == tab) else {
            return;
        };
        let last = tabs.len().saturating_sub(1);
        let chosen = match event.key().as_str() {
            "ArrowRight" if at >= last => 0,
            "ArrowRight" => at.saturating_add(1),
            "ArrowLeft" if at == 0 => last,
            "ArrowLeft" => at.saturating_sub(1),
            "Home" => 0,
            "End" => last,
            _ => return,
        };
        event.prevent_default();
        focused(tabs.get(chosen).cloned());
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _: &leptos::ev::KeyboardEvent = event;
}

/// The number of lines `text` holds, in words.
fn lines(text: &str) -> String {
    match text.lines().count() {
        1 => String::from("1 line"),
        count => format!("{count} lines"),
    }
}

/// Counts the lines of a text area that names its counter, as `event`
/// changes it.
pub(super) fn typed(event: &leptos::ev::Event) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast as _;
        let Some(area) = target(event)
            .and_then(|area| area.dyn_into::<leptos::web_sys::HtmlTextAreaElement>().ok())
        else {
            return;
        };
        let Some(counter) = area
            .get_attribute("data-lines")
            .and_then(|id| document().get_element_by_id(&id))
        else {
            return;
        };
        counter.set_text_content(Some(&lines(&area.value())));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _: &leptos::ev::Event = event;
        let _: fn(&str) -> String = lines;
    }
}

/// Marks the field `field` names as refused, with what is wrong under it,
/// and clears every mark an earlier answer left.
pub(super) fn marked(field: Option<&FieldNote>) {
    #[cfg(target_arch = "wasm32")]
    {
        let document = document();
        while let Some(line) = document
            .query_selector("[data-field-error]:not([hidden])")
            .ok()
            .flatten()
        {
            line.set_text_content(None);
            if line.set_attribute("hidden", "").is_err() {
                leptos::logging::warn!("the browser refused to hide a field's refusal");
                break;
            }
        }
        while let Some(marked) = document.query_selector("[aria-invalid]").ok().flatten() {
            if marked.remove_attribute("aria-invalid").is_err() {
                leptos::logging::warn!("the browser refused to clear a field's mark");
                break;
            }
        }
        if let Some(note) = field {
            if let Some(line) = document.get_element_by_id(&format!("{}-error", note.id)) {
                line.set_text_content(Some(&note.message));
                if line.remove_attribute("hidden").is_err() {
                    leptos::logging::warn!("the browser refused to show a field's refusal");
                }
            }
            if let Some(input) = document.get_element_by_id(&note.id)
                && input.set_attribute("aria-invalid", "true").is_err()
            {
                leptos::logging::warn!("the browser refused to mark a field");
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _: Option<&FieldNote> = field;
}

/// Counts down the deadline of the confirmation the running revision
/// awaits, once a second, while the page shows it.
pub(super) fn countdown() {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(shown) = document().get_element_by_id("confirm-countdown") else {
            return;
        };
        let Some(deadline) = shown
            .get_attribute("data-deadline")
            .and_then(|deadline| deadline.parse::<f64>().ok())
        else {
            return;
        };
        let tick = move || {
            let left = deadline - leptos::web_sys::js_sys::Date::now();
            #[expect(
                clippy::cast_possible_truncation,
                clippy::as_conversions,
                reason = "the milliseconds left until a deadline are far inside i64, and a fraction of one is not shown"
            )]
            let left = left as i64;
            shown.set_text_content(Some(&crate::configure::countdown(left)));
        };
        if let Ok(handle) = set_interval_with_handle(tick, std::time::Duration::from_secs(1)) {
            on_cleanup(move || handle.clear());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::lines;

    #[test]
    fn the_lines_of_an_import_are_counted_in_words() {
        assert_eq!("0 lines", lines(""));
        assert_eq!("1 line", lines("a = 1"));
        assert_eq!("2 lines", lines("a = 1\nb = 2\n"));
    }
}
