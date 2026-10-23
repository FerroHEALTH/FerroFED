// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The messages of the admin actions.

/// Appends these messages to the declaration `$acc`, then hands it to the
/// next macro of the chain `[$next, $rest...]`.
macro_rules! admin_messages {
    ([$next:ident $(, $rest:ident)*] $($acc:tt)*) => {
        $next! {
            [$($rest),*]
            $($acc)*
            /// `The members cannot be sent as a header.`
            MembersCannotSentHeader = "members-cannot-sent-header",
            /// `Name the members to send it to, or * for every active member.`
            NameMembersSendEveryActiveMember = "name-members-send-every-active-member",
            /// `Name the version to send.`
            NameVersionSend = "name-version-send",
            /// `Name the stored query to send.`
            NameStoredQuerySend = "name-stored-query-send",
            /// `This console is configured with no admin listener, so it offers no admin action.`
            ConsoleConfiguredNoAdminListenerSo = "console-configured-no-admin-listener-so",
            /// `drift repair`
            DriftRepair = "drift-repair",
            /// `Choose a replica this console names.`
            ChooseReplicaConsoleNames = "choose-replica-console-names",
            /// `Name the ehr_id to clear.`
            NameEhrIdClear = "name-ehr-id-clear",
            ///  Not every member named took the version; the members below say which.
            NotEveryMemberNamedTookVersion = "not-every-member-named-took-version",
            /// `Incomplete.`
            Incomplete = "incomplete",
            ///  Every member named took the version.
            EveryMemberNamedTookVersion = "every-member-named-took-version",
            /// `Complete.`
            Complete = "complete",
            /// `no ehr_id index entry`
            NoEhrIdIndexEntry = "no-ehr-id-index-entry",
            /// `an ehr_id index entry`
            EhrIdIndexEntry = "ehr-id-index-entry",
            /// `It held nothing for it, which another replica may still hold.`
            HeldNothingWhichAnotherReplicaMay = "held-nothing-which-another-replica-may",
            /// `the ehr_id`
            TheEhrId = "the-ehr-id",
            /// `Members, comma-separated, or * for every active member`
            MembersCommaSeparatedEveryActiveMember = "members-comma-separated-every-active-member",
            /// `Every replica, one after another`
            EveryReplicaOneAfterAnother = "every-replica-one-after-another",
            /// `Replica`
            Replica = "replica",
            /// `Send this version again to the members named? The registry's copy stays as it is.`
            SendVersionAgainMembersNamedRegistry = "send-version-again-members-named-registry",
            /// `Send the version again`
            SendTheVersionAgain = "send-the-version-again",
            /// `Sends the registry's held copy of a version again to the members named.`
            SendsRegistrySHeldCopyVersion = "sends-registry-s-held-copy-version",
            /// `Repair stored-query drift`
            RepairStoredQueryDrift = "repair-stored-query-drift",
            /// `Clear this ehr_id at the replica chosen? Each replica holds its own routing state.`
            ClearEhrIdReplicaChosenEach = "clear-ehr-id-replica-chosen-each",
            /// `Clear the ehr_id`
            ClearTheEhrId = "clear-the-ehr-id",
            /// `Forgets a remedied collision at a replica, so the next request finds the owner again.`
            ForgetsRemediedCollisionReplicaSoNext = "forgets-remedied-collision-replica-so-next",
            /// `Clear a held ehr_id`
            ClearHeldEhrId = "clear-held-ehr-id",
            /// `Running the action.`
            RunningTheAction = "running-the-action",
            /// Confirm the `$what` in its confirmation step to run it.
            ConfirmConfirmationStepRun = "confirm-confirmation-step-run",
            ///  The gateway says: `$said`.
            TheGatewaySays = "the-gateway-says",
            /// Replica `$replica` refused the `$what`: `$status` (`$code`).`$hint`
            ReplicaRefusedThe = "replica-refused-the",
            /// Replica `$replica` did not accept your sign-in for the `$what`: 401 (`$code`).
            ReplicaDidNotAcceptYourSign = "replica-did-not-accept-your-sign",
            /// Replica `$replica`: `$reason`
            Replica2 = "replica-2",
            /// Replica `$replica`: its admin listener gave no answer, so whether the `$what` ran...
            ReplicaAdminListenerGaveNoAnswer = "replica-admin-listener-gave-no-answer",
            /// Replica `$replica`: `$fault`.
            Replica3 = "replica-3",
            /// It held a collision between `$line`.
            HeldCollisionBetween = "held-collision-between",
            /// It held one owner, `$line`.
            ItHeldOneOwner = "it-held-one-owner",
            /// Replica `$replica` cleared `$ehr_id`.
            ReplicaCleared = "replica-cleared",
            ///  `$held` It forgot `$index` and the bindings of `$sessions` sessions.
            ForgotBindingsSessions = "forgot-bindings-sessions",
            ///  The registry's copy: `$registry`.
            TheRegistrySCopy = "the-registry-s-copy",
            /// Replica `$replica` sent `$name` version `$version` again: `$status`.`$registry`
            ReplicaSentVersionAgain = "replica-sent-version-again",
            /// It goes to replica `$name`, the first one configured: the replicas share one...
            GoesReplicaFirstOneConfiguredReplicas = "goes-replica-first-one-configured-replicas",
        }
    };
}
