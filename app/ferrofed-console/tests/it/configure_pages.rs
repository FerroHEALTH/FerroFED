// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The configuration page's parts against a stub admin listener: each tab
//! and form at a path of its own, each member marked with its owner, an
//! edit offered only for a console member, a path that names no part
//! refused, and every value the gateway answers escaped on every part.

use std::error::Error;

use crate::configure::{changes, listener, page, signed_in_console};

#[tokio::test(flavor = "multi_thread")]
async fn the_page_marks_each_owner_and_offers_an_edit_only_for_a_console_member()
-> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    let (service, session) = signed_in_console(&listener)?;
    for path in ["/configuration", "/configuration/members"] {
        let body = page(&service, &session, path).await?;
        assert!(body.contains("<title>Configuration · "), "{body}");
        assert!(body.contains("File, read-only"), "{body}");
        assert!(body.contains(">pending<"), "{body}");
        assert!(
            body.contains(r#"href="/configuration/members/node-c""#),
            "{body}"
        );
        assert!(
            body.contains(r#"aria-label="Actions for node-c""#),
            "a console member's row has its row menu: {body}"
        );
        assert!(
            body.contains(r#"<a role="menuitem" href="/configuration/members/node-c""#),
            "whose Edit opens the member's form: {body}"
        );
        assert!(
            body.contains(r#"commandfor="cfg-remove-node-c-dialog""#),
            "a console member's row offers its own confirmed removal: {body}"
        );
        assert!(
            !body.contains("cfg-remove-node-a-dialog"),
            "a member the file provides offers none: {body}"
        );
        assert!(
            body.contains(r#"command="show-modal" commandfor="config-how""#),
            "{body}"
        );
        assert!(body.contains(r#"<dialog id="config-how""#), "{body}");
        assert!(body.contains("Revision 1 running"), "{body}");
        assert!(body.contains("Revision 2 saved, not applied"), "{body}");
        assert!(body.contains("Apply revision 2"), "{body}");
        assert!(body.contains("What revision 2 changes, by key"), "{body}");
        assert!(body.contains("registry.members[0].node.id"), "{body}");
        assert!(
            body.contains(r#"aria-current="page" data-tab="tab-members""#),
            "{body}"
        );
    }
    let edit = page(&service, &session, "/configuration/members/node-c").await?;
    assert!(edit.contains("Edit member node-c"), "{edit}");
    assert!(edit.contains(r#"name="form[query][0][url]""#), "{edit}");
    assert!(edit.contains(r#"commandfor="remove-member""#), "{edit}");
    assert!(
        edit.contains(r#"class="fui-repeater-card""#),
        "each endpoint is an F7 sub-card: {edit}"
    );
    assert!(
        edit.contains(r#"<template data-repeat="blank" data-token="__row__">"#),
        "Add copies a blank endpoint in place with WebAssembly: {edit}"
    );
    assert!(
        edit.contains(r#"<template data-repeat="blank" data-token="__profile__">"#),
        "a blank FHIR endpoint's own profiles add rows in place under a token of their own: {edit}"
    );
    assert!(
        edit.contains(r#"name="form[edit]" value="add-query""#),
        "and posts the form without it: {edit}"
    );
    assert!(
        edit.contains(r#"name="form[node]" value="node-c""#),
        "{edit}"
    );
    let detail = page(&service, &session, "/configuration/members/node-a").await?;
    assert!(!detail.contains("Edit member node-a"), "{detail}");
    assert!(
        detail.contains("registry document provides this member"),
        "{detail}"
    );
    assert!(
        !detail.contains(r#"commandfor="remove-member""#),
        "{detail}"
    );
    let queries = page(&service, &session, "/configuration/stored-queries").await?;
    assert!(queries.contains("Store, immutable"), "{queries}");
    let history = page(&service, &session, "/configuration/history").await?;
    assert!(
        !history.contains("rollback-confirm"),
        "no revision ran before 1: {history}"
    );
    assert_eq!(
        0,
        changes(&listener).await?,
        "loading the page changes nothing"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_path_that_names_no_part_of_the_page_is_refused() -> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    let (service, session) = signed_in_console(&listener)?;
    let body = page(&service, &session, "/configuration/revisions").await?;
    assert!(
        body.contains("This path names no part of the configuration page."),
        "{body}"
    );
    let body = page(&service, &session, "/configuration/members/node-z").await?;
    assert!(body.contains("No member has the node_id node-z."), "{body}");
    Ok(())
}

// NOTE: no specification governs this: our own design; every value the gateway
// answers reaches the page as text, the obligation of an `inner_html` sink.
#[tokio::test(flavor = "multi_thread")]
async fn every_value_the_gateway_answers_is_escaped_on_the_page() -> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    let (service, session) = signed_in_console(&listener)?;
    let mut body = String::new();
    for path in [
        "/configuration/members",
        "/configuration/members/node-c",
        "/configuration/stored-queries",
        "/configuration/history",
        "/configuration/history?compare=2",
    ] {
        body.push_str(&page(&service, &session, path).await?);
    }
    for raw in [
        "<script>alert(1)",
        "<script>x</script>",
        "<b>x</b>",
        "<b>1</b>",
        "<i>q</i>",
        "\"node-c\"<script>",
        "\" onmouseover=\"x",
    ] {
        assert!(!body.contains(raw), "{raw} reached the page raw: {body}");
    }
    assert!(
        body.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
        "{body}"
    );
    assert!(body.contains("&lt;i&gt;q&lt;/i&gt;"), "{body}");
    assert!(
        body.contains("add C &lt;script&gt;x&lt;/script&gt;"),
        "{body}"
    );
    Ok(())
}
