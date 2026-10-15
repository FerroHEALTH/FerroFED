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

use leptos::prelude::*;

use crate::admin::act::server::EVERY_REPLICA;
use crate::config::settings::ReplicaSettings;

/// The collision clear's fields: the `ehr_id`, offered from the incidents'
/// `ehr_ids`, the replica among `replicas` or every one, and the
/// confirmation.
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
    view! {
        <p>
            "Forgets the ehr_id from a replica's ehr_id index and from the resolution "
            "bindings of every session there, so the next request finds its owner again. "
            "Clear it once the members' operators have remedied the collision. Each "
            "replica holds its own routing state."
        </p>
        <p>
            <label for="clear-ehr-id">"ehr_id"</label>
            <input
                type="text"
                id="clear-ehr-id"
                name="form[ehr_id]"
                list="clear-ehr-ids"
                autocomplete="off"
                spellcheck="false"
                required
            />
            <datalist id="clear-ehr-ids">{known}</datalist>
        </p>
        <p>
            <label for="clear-replica">"Replica"</label>
            <select id="clear-replica" name="form[replica]">
                <option value=EVERY_REPLICA>"Every replica, one after another"</option>
                {choices}
            </select>
        </p>
        <p>
            <input type="checkbox" id="clear-confirm" name="form[confirm]" value="yes" required />
            <label for="clear-confirm">
                "I confirm the clear of this ehr_id at the replica chosen"
            </label>
        </p>
    }
    .to_html()
}

/// The drift repair's fields: the version, offered from the stored
/// queries' `names`, the members, and the confirmation, with the replica it
/// goes to.
#[must_use]
pub fn repair(replica: &ReplicaSettings, names: &[String]) -> String {
    let known = names
        .iter()
        .map(|name| view! { <option value=name.clone()></option> })
        .collect_view();
    let goes = format!(
        "Sends the registry's held copy of a version again to the members named, and leaves the registry's copy as it is. It goes to replica {}, the first one configured: the replicas share one stored-query store.",
        replica.name
    );
    view! {
        <p>{goes}</p>
        <p>
            <label for="repair-name">"Stored query name"</label>
            <input
                type="text"
                id="repair-name"
                name="form[name]"
                list="repair-names"
                autocomplete="off"
                required
            />
            <datalist id="repair-names">{known}</datalist>
            " "
            <label for="repair-version">"Version"</label>
            <input
                type="text"
                id="repair-version"
                name="form[version]"
                autocomplete="off"
                required
            />
        </p>
        <p>
            <label for="repair-endpoints">
                "Members, comma-separated, or * for every active member"
            </label>
            <input type="text" id="repair-endpoints" name="form[endpoints]" value="*" required />
        </p>
        <p>
            <input type="checkbox" id="repair-confirm" name="form[confirm]" value="yes" required />
            <label for="repair-confirm">
                "I confirm sending this version again to the members named"
            </label>
        </p>
    }
    .to_html()
}
