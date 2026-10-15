// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The page's tables: the revisions with the preview of the newest,
//! the members, the stored queries and the history.

use ferrofed_registry::console::{MemberResource, QueryRegistry, QueryResource};
use ferrofed_registry::operator::Page;
use ferrofed_registry::revision::{Difference, Origin, Revision, RevisionId, Standing};
use leptos::prelude::*;

use crate::config::settings::ReplicaSettings;
use crate::configure::render::{author, mark_said};
use crate::configure::sections::{owner, state};

/// Where a revision came from, in words.
fn origin(origin: Origin) -> &'static str {
    match origin {
        Origin::File => "file",
        Origin::Operator => "operator",
        Origin::Rollback => "rollback",
        Origin::Import => "import, a dry run until applied",
        _ => "another origin",
    }
}

/// The revisions: which runs, which is newest, and what the newest changes
/// when it does not run.
pub(super) fn revisions_section(
    replica: &ReplicaSettings,
    standing: &Standing,
    pending: Option<&[Difference]>,
) -> AnyView {
    let running = standing
        .current
        .as_ref()
        .map_or_else(|| String::from("No revision is marked running."), mark_said);
    let newest = standing.head.map_or_else(
        || String::from("The store holds no revision."),
        |head| format!("The newest revision is {head}."),
    );
    let preview = match (standing.head, pending) {
        (Some(head), Some(changes)) => {
            let running = standing
                .current
                .as_ref()
                .map(|mark| mark.revision.to_string())
                .unwrap_or_default();
            view! {
                <p role="status">
                    {format!(
                        "Revision {head} is saved and not applied. What it changes against revision {running}:",
                    )}
                </p>
                {diff_table(changes, head)}
            }
            .into_any()
        }
        _ => ().into_any(),
    };
    let goes = format!(
        "Changes go to replica {}, the first one configured: the replicas share one revision store, and each applies the revision marked running.",
        replica.name
    );
    view! {
        <section aria-labelledby="config-revisions">
            <h2 id="config-revisions">"Revisions"</h2>
            <p>{running}</p>
            <p>{newest}</p>
            {preview}
            {crate::configure::sections::notices(standing)}
            <p>{goes}</p>
        </section>
    }
    .into_any()
}

/// The keys `changes` names, against revision `to`.
pub(super) fn diff_table(changes: &[Difference], to: RevisionId) -> AnyView {
    if changes.is_empty() {
        return view! { <p>"It changes no setting."</p> }.into_any();
    }
    let rows = changes
        .iter()
        .map(|change| {
            view! {
                <tr>
                    <th scope="row">
                        <code>{change.key.clone()}</code>
                    </th>
                    <td>{change.change.as_str()}</td>
                    <td>
                        <code>{change.from.clone().unwrap_or_default()}</code>
                    </td>
                    <td>
                        <code>{change.to.clone().unwrap_or_default()}</code>
                    </td>
                </tr>
            }
        })
        .collect_view();
    view! {
        <div class=crate::styles::TABLE_SCROLL tabindex="0">
            <table>
                <caption>{format!("What revision {to} changes, by key")}</caption>
                <thead>
                    <tr>
                        <th scope="col">"Key"</th>
                        <th scope="col">"Change"</th>
                        <th scope="col">"Running value"</th>
                        <th scope="col">{format!("Value in revision {to}")}</th>
                    </tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
        </div>
    }
    .into_any()
}

/// The members, each with its owner.
pub(super) fn members_section(members: &[MemberResource]) -> AnyView {
    let rows = members
        .iter()
        .map(|member| {
            let endpoints = member
                .endpoints
                .iter()
                .map(ferrofed_registry::id::EndpointId::as_str)
                .collect::<Vec<_>>()
                .join(", ");
            view! {
                <tr>
                    <th scope="row">{member.node.as_str().to_owned()}</th>
                    <td>{member.organisation.as_str().to_owned()}</td>
                    <td>{member.system_id.as_str().to_owned()}</td>
                    <td>{endpoints}</td>
                    <td>{owner(member.owner)}</td>
                    <td>{state(member.state)}</td>
                </tr>
            }
        })
        .collect_view();
    view! {
        <section aria-labelledby="config-members">
            <h2 id="config-members">"Members"</h2>
            <p>
                "A member the file's registry document or a care services directory provides "
                "is read-only here: change it in its source. A console member is changed "
                "through a revision you preview and then apply."
            </p>
            <div class=crate::styles::TABLE_SCROLL tabindex="0">
                <table>
                    <caption>"Members and their owners"</caption>
                    <thead>
                        <tr>
                            <th scope="col">"Node"</th>
                            <th scope="col">"Organisation"</th>
                            <th scope="col">"system_id"</th>
                            <th scope="col">"Endpoints"</th>
                            <th scope="col">"Owner"</th>
                            <th scope="col">"State"</th>
                        </tr>
                    </thead>
                    <tbody>{rows}</tbody>
                </table>
            </div>
        </section>
    }
    .into_any()
}

/// The stored-query versions, each with its owner.
pub(super) fn queries_section(registry: QueryRegistry, queries: &[QueryResource]) -> AnyView {
    if registry == QueryRegistry::Absent {
        return view! {
            <section aria-labelledby="config-queries">
                <h2 id="config-queries">"Stored queries"</h2>
                <p>"The gateway offers no stored-query registry."</p>
            </section>
        }
        .into_any();
    }
    let rows = queries
        .iter()
        .map(|query| {
            view! {
                <tr>
                    <th scope="row">{query.name.clone()}</th>
                    <td>{query.version.clone()}</td>
                    <td>{owner(query.owner)}</td>
                </tr>
            }
        })
        .collect_view();
    let read_only = (registry == QueryRegistry::ReadOnly).then(|| {
        view! {
            <p>
                "The stored queries are read from the read-only definition files, so the "
                "console stores no version: change the files."
            </p>
        }
    });
    view! {
        <section aria-labelledby="config-queries">
            <h2 id="config-queries">"Stored queries"</h2>
            <p>
                "A stored version is immutable (§12.7): a change is a new version, and "
                "none is edited or removed."
            </p>
            {read_only}
            <div class=crate::styles::TABLE_SCROLL tabindex="0">
                <table>
                    <caption>"Stored-query versions and their owners"</caption>
                    <thead>
                        <tr>
                            <th scope="col">"Name"</th>
                            <th scope="col">"Version"</th>
                            <th scope="col">"Owner"</th>
                        </tr>
                    </thead>
                    <tbody>{rows}</tbody>
                </table>
            </div>
        </section>
    }
    .into_any()
}

/// The newest revisions, newest first.
pub(super) fn history_section(history: &Page<Revision>) -> AnyView {
    let rows = history
        .items
        .iter()
        .map(|revision| {
            let origin = match revision.restores {
                Some(restored) => format!("rollback to revision {restored}"),
                None => origin(revision.origin).to_owned(),
            };
            view! {
                <tr>
                    <th scope="row">{revision.id.to_string()}</th>
                    <td>{origin}</td>
                    <td>{author(&revision.author)}</td>
                    <td>{revision.at.to_string()}</td>
                    <td>{revision.reason.clone()}</td>
                    <td>
                        <a href=crate::configure::export::href(revision.id) rel="external">
                            {format!("Export revision {} as TOML", revision.id)}
                        </a>
                    </td>
                </tr>
            }
        })
        .collect_view();
    let shown = format!(
        "The {} newest of {} revisions, newest first",
        history.items.len(),
        history.total
    );
    view! {
        <section aria-labelledby="config-history">
            <h2 id="config-history">"History"</h2>
            <div class=crate::styles::TABLE_SCROLL tabindex="0">
                <table>
                    <caption>{shown}</caption>
                    <thead>
                        <tr>
                            <th scope="col">"Revision"</th>
                            <th scope="col">"Origin"</th>
                            <th scope="col">"Made by"</th>
                            <th scope="col">"At"</th>
                            <th scope="col">"Reason"</th>
                            <th scope="col">"TOML"</th>
                        </tr>
                    </thead>
                    <tbody>{rows}</tbody>
                </table>
            </div>
        </section>
    }
    .into_any()
}
