// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The fields of each admin action form, rendered on the server.
//!
//! The fields are static markup the browser never changes, so the server
//! renders them once into the view it sends, and the page shows that HTML
//! inside the form it hydrates, so their code stays out of the browser's
//! bundle. Leptos escapes every text
//! and attribute value they carry: a replica name, an `ehr_id` an incident
//! names, a stored query's name.

use ferrohealth_ui::view::form::{INPUT, SELECT, field};
use ferrohealth_ui::view::layout::{FieldRow, field_row};
use leptos::prelude::*;

use crate::admin::act::server::EVERY_REPLICA;
use crate::config::settings::ReplicaSettings;

/// The collision clear's fields: the `ehr_id`, offered from the incidents'
/// `ehr_ids`, and the replica among `replicas` or every one; the card adds
/// the confirmation step.
#[must_use]
pub fn clear(replicas: &[ReplicaSettings], ehr_ids: &[String]) -> String {
    let known = ehr_ids
        .iter()
        .map(|ehr_id| view! { <option value=ehr_id.clone()></option> })
        .collect_view();
    let choices = replicas
        .iter()
        .map(|replica| {
            view! { <option value=replica.name.clone()>{replica.name.clone()}</option> }
        })
        .collect_view();
    field_row(
        FieldRow::Halves,
        view! {
            {field(
                "clear-ehr-id",
                "ehr_id",
                None,
                view! {
                    <input
                        type="text"
                        id="clear-ehr-id"
                        name="form[ehr_id]"
                        class=INPUT
                        list="clear-ehr-ids"
                        autocomplete="off"
                        spellcheck="false"
                        required
                    />
                    <datalist id="clear-ehr-ids">{known}</datalist>
                },
            )}
            {field(
                "clear-replica",
                "Replica",
                None,
                view! {
                    <select id="clear-replica" name="form[replica]" class=SELECT>
                        <option value=EVERY_REPLICA>"Every replica, one after another"</option>
                        {choices}
                    </select>
                },
            )}
        },
    )
    .to_html()
}

/// The drift repair's fields: the version, offered from the stored
/// queries' `names`, and the members, with the replica it goes to; the
/// card adds the confirmation step.
#[must_use]
pub fn repair(replica: &ReplicaSettings, names: &[String]) -> String {
    let known = names
        .iter()
        .map(|name| view! { <option value=name.clone()></option> })
        .collect_view();
    let goes = format!(
        "It goes to replica {}, the first one configured: the replicas share one stored-query store.",
        replica.name
    );
    view! {
        <div class="fui-form">
            <p class="fui-muted">{goes}</p>
            {field_row(
                FieldRow::NarrowWide,
                view! {
                    {field(
                        "repair-version",
                        "Version",
                        None,
                        view! {
                            <input
                                type="text"
                                id="repair-version"
                                name="form[version]"
                                class=INPUT
                                autocomplete="off"
                                required
                            />
                        },
                    )}
                    {field(
                        "repair-name",
                        "Stored query name",
                        None,
                        view! {
                            <input
                                type="text"
                                id="repair-name"
                                name="form[name]"
                                class=INPUT
                                list="repair-names"
                                autocomplete="off"
                                required
                            />
                            <datalist id="repair-names">{known}</datalist>
                        },
                    )}
                },
            )}
            {field(
                "repair-endpoints",
                "Members, comma-separated, or * for every active member",
                None,
                view! {
                    <input
                        type="text"
                        id="repair-endpoints"
                        name="form[endpoints]"
                        class=INPUT
                        value="*"
                        required
                    />
                },
            )}
        </div>
    }
    .to_html()
}
