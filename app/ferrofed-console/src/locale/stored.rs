// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The messages of `locales/en/stored.ftl`.

messages! {
    /// `Send again`
    RepairAction = "repair-action",
    /// `Version … was sent again by …, and every endpoint took it.`
    RepairDone = "repair-done",
    /// `Endpoints`
    RepairEndpoints = "repair-endpoints",
    /// `* for every active endpoint, or endpoint ids separated by commas.`
    RepairEndpointsHelp = "repair-endpoints-help",
    /// `The endpoints cannot be sent as a header.`
    RepairEndpointsIllegal = "repair-endpoints-illegal",
    /// `Name the version to send again.`
    RepairNameVersion = "repair-name-version",
    /// `The console is configured with no admin listener, so it offers no …`
    RepairNoAdmin = "repair-no-admin",
    /// `Confirm the drift repair in its confirmation step to run it.`
    RepairConfirm = "repair-confirm",
    /// `drift repair`
    RepairWhat = "repair-what",
    /// `Version … was sent again by …, and some endpoints did not take it.`
    RepairPartly = "repair-partly",
    /// `Sent by replica …; the stored-query store is the replicas' shared one.`
    RepairReplica = "repair-replica",
    /// `Send again`
    RepairSubmit = "repair-submit",
    /// `Send a version again to the endpoints`
    RepairTitle = "repair-title",
    /// `The AQL of version …`
    StoredAqlOf = "stored-aql-of",
    /// `The gateway's own namespace; the console cannot change it`
    StoredGatewayOwned = "stored-gateway-owned",
    /// `New stored query version`
    StoredNewVersion = "stored-new-version",
    /// `The gateway holds no stored query.`
    StoredNone = "stored-none",
    /// `Read-only`
    StoredReadOnly = "stored-read-only",
    /// `Read from the definition files; the console cannot change it`
    StoredReadOnlyTitle = "stored-read-only-title",
    /// `Run in the query console`
    StoredRun = "stored-run",
    /// `The queries the gateway holds, by namespace, each with its versions.`
    StoredSubtitle = "stored-subtitle",
    /// `Version … of …`
    StoredSubtitleOne = "stored-subtitle-one",
    /// `AQL`
    StoredTabAql = "stored-tab-aql",
    /// `Versions`
    StoredTabVersions = "stored-tab-versions",
    /// `Stored queries`
    StoredTitle = "stored-title",
    /// `{ $count ->`
    StoredVersionCount = "stored-version-count",
    /// `Version … is not held; the newest is shown.`
    StoredVersionNotHeld = "stored-version-not-held",
}
