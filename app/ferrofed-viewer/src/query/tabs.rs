// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Two tabs over two panels, in the ARIA tabs pattern: the arrow keys, Home
//! and End move between the tabs, the tab in focus is the one shown, and
//! only it is in the tab order (WAI-ARIA Authoring Practices 1.2, Tabs).

use leptos::prelude::*;

use crate::styles;

/// The two tabs of one tab list.
#[derive(Debug, Clone, Copy)]
pub(super) struct TabPair {
    /// What the tab list is for.
    pub label: &'static str,
    /// The ids of the two tabs.
    pub tabs: [&'static str; 2],
    /// The ids of the two panels, in the order of the tabs.
    pub panels: [&'static str; 2],
    /// What each tab says.
    pub names: [&'static str; 2],
}

/// The tab list of `pair`, its second tab shown while `second` holds.
pub(super) fn tab_list(pair: TabPair, second: RwSignal<bool>) -> impl IntoView {
    let first_tab = NodeRef::<leptos::html::Button>::new();
    let second_tab = NodeRef::<leptos::html::Button>::new();
    let keys = move |event: leptos::ev::KeyboardEvent| {
        let to_second = match event.key().as_str() {
            "ArrowLeft" | "ArrowRight" => !second.get_untracked(),
            "Home" => false,
            "End" => true,
            _ => return,
        };
        event.prevent_default();
        second.set(to_second);
        focus(if to_second { second_tab } else { first_tab });
    };
    let [first_id, second_id] = pair.tabs;
    let [first_panel, second_panel] = pair.panels;
    let [first_name, second_name] = pair.names;
    view! {
        <div role="tablist" aria-label=pair.label class=styles::TAB_LIST on:keydown=keys>
            <button
                type="button"
                role="tab"
                id=first_id
                aria-controls=first_panel
                aria-selected=move || (!second.get()).to_string()
                tabindex=move || if second.get() { "-1" } else { "0" }
                class=styles::TAB_BUTTON
                node_ref=first_tab
                on:click=move |_| second.set(false)
            >
                {first_name}
            </button>
            <button
                type="button"
                role="tab"
                id=second_id
                aria-controls=second_panel
                aria-selected=move || second.get().to_string()
                tabindex=move || if second.get() { "0" } else { "-1" }
                class=styles::TAB_BUTTON
                node_ref=second_tab
                on:click=move |_| second.set(true)
            >
                {second_name}
            </button>
        </div>
    }
}

/// Moves the focus to `tab`.
fn focus(tab: NodeRef<leptos::html::Button>) {
    if let Some(tab) = tab.get_untracked()
        && tab.focus().is_err()
    {
        leptos::logging::warn!("the browser refused the focus on a tab");
    }
}
