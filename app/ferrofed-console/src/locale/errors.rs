// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The messages of `locales/en/errors.ftl`.

messages! {
    /// `The gateway's operator surface needs a token that carries its …`
    ForbiddenBody = "forbidden-body",
    /// `Whoever runs your identity provider can grant the scope.`
    ForbiddenHint = "forbidden-hint",
    /// `No operator scope`
    ForbiddenTitle = "forbidden-title",
    /// `The console has no page at …. The address may be mistyped, or what …`
    NotFoundBody = "not-found-body",
    /// `Back to the dashboard`
    NotFoundDashboard = "not-found-dashboard",
    /// `Organisations`
    NotFoundOrganisations = "not-found-organisations",
    /// `Not found`
    NotFoundTitle = "not-found-title",
    /// `The console cannot tell whether you are signed in, so it shows …`
    UnreadableBody = "unreadable-body",
    /// `The console cannot read its sessions`
    UnreadableTitle = "unreadable-title",
}
