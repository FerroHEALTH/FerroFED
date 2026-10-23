// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The messages of `locales/en/integrity.ftl`.

messages! {
    /// `Counts since the start`
    IntegrityCountsTitle = "integrity-counts-title",
    /// `No incident is open.`
    IntegrityNoneOpen = "integrity-none-open",
    /// `No incident has been resolved.`
    IntegrityNoneResolved = "integrity-none-resolved",
    /// `Open`
    IntegrityOpen = "integrity-open",
    /// `{ $count ->`
    IntegrityOpenCount = "integrity-open-count",
    /// `Resolved`
    IntegrityResolved = "integrity-resolved",
    /// `{ $count ->`
    IntegrityResolvedCount = "integrity-resolved-count",
    /// `The incidents the replica keeps, and the counts of each kind since …`
    IntegritySubtitle = "integrity-subtitle",
    /// `Integrity`
    IntegrityTitle = "integrity-title",
    /// `Two nodes claim one EHR id`
    KindEhrIdCollision = "kind-ehr-id-collision",
    /// `An EHR id was indexed at a second node`
    KindIndexInsertCollision = "kind-index-insert-collision",
    /// `One creating system seen at two nodes`
    KindLearnedConflict = "kind-learned-conflict",
    /// `Two nodes hold one prescription identifier`
    KindPrescriptionCollision = "kind-prescription-collision",
    /// `A registered creating system routes to two nodes`
    KindRegisteredConflict = "kind-registered-conflict",
}
