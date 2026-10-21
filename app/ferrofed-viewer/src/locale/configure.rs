// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The messages of the configuration page and its tabs: members, credentials
//! and bindings, stored queries, history, import and export.

/// Appends these messages to the declaration `$acc`, then hands it to the
/// next macro of the chain `[$next, $rest...]`.
macro_rules! configure_messages {
    ([$next:ident $(, $rest:ident)*] $($acc:tt)*) => {
        $next! {
            [$($rest),*]
            $($acc)*
            /// `A member with this node_id exists: change it with its own edit form.`
            MemberNodeIdExistsChangeOwn = "member-node-id-exists-change-own",
            /// `This path names no part of the configuration page.`
            PathNamesNoPartConfigurationPage = "path-names-no-part-configuration-page",
            /// `This console is configured with no admin listener, so it shows no configuration.`
            ConsoleConfiguredNoAdminListenerSo2 = "console-configured-no-admin-listener-so-2",
            /// `1 line`
            N1Line = "n-1-line",
            /// `This console is configured with no admin listener, so it exports no configuration.`
            ConsoleConfiguredNoAdminListenerSo3 = "console-configured-no-admin-listener-so-3",
            /// `The path names no revision number.`
            PathNamesNoRevisionNumber = "path-names-no-revision-number",
            /// `The session store could not be read.`
            SessionStoreCouldNotRead = "session-store-could-not-read",
            /// `text/plain; charset=utf-8`
            TextPlainCharsetUtf8 = "text-plain-charset-utf-8",
            /// `FHIR`
            Fhir = "fhir",
            /// `Query API`
            QueryApi = "query-api",
            /// `Give at least one Query API endpoint.`
            GiveLeastOneQueryApiEndpoint = "give-least-one-query-api-endpoint",
            /// `The system_id is not an openEHR UID.`
            SystemIdNotOpenehrUid = "system-id-not-openehr-uid",
            /// `1 to 64 letters, digits, '.', '-' or '_', starting with a letter or digit`
            N164LettersDigitsStartingLetter = "n-1-64-letters-digits-starting-letter",
            /// `The form's endpoint rows do not read; reload the page.`
            FormSEndpointRowsDoNot = "form-s-endpoint-rows-do-not",
            /// `Configuration changes`
            ConfigurationChanges = "configuration-changes",
            /// `Sending the change.`
            SendingTheChange = "sending-the-change",
            /// `Loading the configuration.`
            LoadingTheConfiguration = "loading-the-configuration",
            /// `The deadline has passed`
            TheDeadlineHasPassed = "the-deadline-has-passed",
            /// `Choose the credential or binding section to remove.`
            ChooseCredentialBindingSectionRemove = "choose-credential-binding-section-remove",
            /// `Paste the TOML to import.`
            PasteTomlImport = "paste-toml-import",
            /// `base revision`
            BaseRevision = "base-revision",
            /// `The form names no action this page offers.`
            FormNamesNoActionPageOffers = "form-names-no-action-page-offers",
            /// `revision to approve`
            RevisionToApprove = "revision-to-approve",
            /// `revision to confirm`
            RevisionToConfirm = "revision-to-confirm",
            /// `Choose the binding section.`
            ChooseTheBindingSection = "choose-the-binding-section",
            /// `Choose how the gateway reaches the endpoint.`
            ChooseHowGatewayReachesEndpoint = "choose-how-gateway-reaches-endpoint",
            /// `The endpoint_id is not 1 to 64 letters, digits, '.', '-' or '_'.`
            EndpointIdNot164Letters = "endpoint-id-not-1-64-letters",
            /// `Choose the endpoint of the credentials.`
            ChooseEndpointCredentials = "choose-endpoint-credentials",
            /// `Give the AQL to store.`
            GiveAqlStore = "give-aql-store",
            /// `Name the version to store.`
            NameVersionStore = "name-version-store",
            /// `Name the stored query.`
            NameTheStoredQuery = "name-the-stored-query",
            /// `running revision to roll back`
            RunningRevisionRollBack = "running-revision-roll-back",
            /// `revision to apply`
            RevisionToApply = "revision-to-apply",
            /// `Choose the console member to remove.`
            ChooseConsoleMemberRemove = "choose-console-member-remove",
            /// `Confirm the change in its confirmation step to run it.`
            ConfirmChangeConfirmationStepRun = "confirm-change-confirmation-step-run",
            /// `The reason is longer than 1000 characters.`
            ReasonLongerThan1000Characters = "reason-longer-than-1000-characters",
            /// `Give the reason for the change.`
            GiveReasonChange = "give-reason-change",
            /// `Credentials`
            Credentials = "credentials",
            ///  It is not applied: apply it from the card at the top once you have read its changes.
            NotAppliedApplyFromCardTop = "not-applied-apply-from-card-top",
            ///  It awaits your confirmation: confirm it in the card at the top, or roll it back.
            AwaitsYourConfirmationConfirmCardTop = "awaits-your-confirmation-confirm-card-top",
            /// `The preview of its changes could not be read; reload the page to see it in the card at...`
            PreviewChangesCouldNotReadReload = "preview-changes-could-not-read-reload",
            /// `Each setting is its own field; a number or true or false is read as one.`
            EachSettingOwnFieldNumberTrue = "each-setting-own-field-number-true",
            /// `Section`
            Section = "section",
            /// `Add a binding section`
            AddABindingSection = "add-a-binding-section",
            /// `Add secret`
            AddSecret = "add-secret",
            /// `Secrets`
            Secrets = "secrets",
            /// `Add setting`
            AddSetting = "add-setting",
            /// `Settings`
            Settings = "settings",
            /// `Binding section`
            BindingSection = "binding-section",
            /// `Remove this new secret`
            RemoveThisNewSecret = "remove-this-new-secret",
            /// `Value, never shown again`
            ValueNeverShownAgain = "value-never-shown-again",
            /// `Such as oauth2.client_secret.`
            SuchOauth2ClientSecret = "such-oauth2-client-secret",
            /// `Secret key`
            SecretKey = "secret-key",
            /// `this new setting`
            ThisNewSetting = "this-new-setting",
            /// `Its parts joined by dots, such as oauth2.scope.`
            PartsJoinedDotsSuchOauth2Scope = "parts-joined-dots-such-oauth2-scope",
            /// `Key`
            Key = "key",
            /// `Credentials and bindings`
            CredentialsAndBindings = "credentials-and-bindings",
            /// `Saved as a revision; once applied it awaits a confirmation, or the gateway rolls it back.`
            SavedRevisionOnceAppliedAwaitsConfirmation = "saved-revision-once-applied-awaits-confirmation",
            /// `TLS`
            Tls = "tls",
            /// `Grant settings`
            GrantSettings = "grant-settings",
            /// `Endpoint and grant`
            EndpointAndGrant = "endpoint-and-grant",
            /// `Add onward credentials`
            AddOnwardCredentials = "add-onward-credentials",
            /// `How the gateway reaches the endpoint; the form shows that grant's fields.`
            HowGatewayReachesEndpointFormShows = "how-gateway-reaches-endpoint-form-shows",
            /// `Grant type`
            GrantType = "grant-type",
            /// `The endpoints the members declare that hold no credentials yet.`
            EndpointsMembersDeclareHoldNoCredentials = "endpoints-members-declare-hold-no-credentials",
            /// `Endpoint`
            Endpoint2 = "endpoint-2",
            /// `An absolute path on the gateway's host.`
            AbsolutePathGatewaySHost = "absolute-path-gateway-s-host",
            /// `Add value`
            AddValue = "add-value",
            /// `Remove this value`
            RemoveThisValue = "remove-this-value",
            /// `Absolute path of the file it is read from`
            AbsolutePathFileReadFrom = "absolute-path-file-read-from",
            /// `Clear`
            Clear = "clear",
            /// `Read from a file`
            ReadFromAFile = "read-from-a-file",
            /// `Set a value`
            SetAValue = "set-a-value",
            /// `Leave unset`
            LeaveUnset = "leave-unset",
            /// `Keep`
            Keep = "keep",
            /// `Not set.`
            NotSet = "not-set",
            /// `Credentials and binding sections and their owners`
            CredentialsBindingSectionsTheirOwners = "credentials-binding-sections-their-owners",
            /// `Add credentials`
            AddCredentials = "add-credentials",
            /// `A credential or a binding section the file provides is read-only here; a secret shows...`
            CredentialBindingSectionFileProvidesRead = "credential-binding-section-file-provides-read",
            /// `Add binding section`
            AddBindingSection = "add-binding-section",
            /// `No onward credential or binding section is set.`
            NoOnwardCredentialBindingSectionSet = "no-onward-credential-binding-section-set",
            /// `Actions`
            Actions = "actions",
            /// `Owner`
            Owner = "owner",
            /// `Resource`
            Resource = "resource",
            /// `Edit`
            Edit = "edit",
            /// `Reason`
            Reason = "reason",
            /// `Save the removal`
            SaveTheRemoval = "save-the-removal",
            /// `Remove`
            Remove = "remove",
            /// `1 setting`
            N1Setting = "n-1-setting",
            /// `no settings`
            NoSettings = "no-settings",
            /// `not readable`
            NotReadable = "not-readable",
            /// `: set, read from`
            SetReadFrom = "set-read-from",
            /// `not set`
            NotSet2 = "not-set-2",
            /// `Paste the TOML an export wrote, or one kept in version control.`
            PasteTomlExportWroteOneKept = "paste-toml-export-wrote-one-kept",
            /// `Import`
            Import = "import",
            /// `Import as a dry-run revision`
            ImportDryRunRevision = "import-dry-run-revision",
            /// `0 lines`
            N0Lines = "n-0-lines",
            /// `TOML`
            Toml = "toml",
            /// `The configuration less its bootstrap core, each secret as its file reference alone.`
            ConfigurationLessBootstrapCoreEachSecret = "configuration-less-bootstrap-core-each-secret",
            /// `Export`
            Export = "export",
            /// `No revision runs, so none downloads.`
            NoRevisionRunsSoNoneDownloads = "no-revision-runs-so-none-downloads",
            /// `Download as TOML`
            DownloadAsToml = "download-as-toml",
            /// `Close the comparison`
            CloseTheComparison = "close-the-comparison",
            /// `Every revision, who made it and why; each one downloads as TOML.`
            EveryRevisionWhoMadeWhyEach = "every-revision-who-made-why-each",
            /// `History`
            History = "history",
            /// `Compare with running`
            CompareWithRunning = "compare-with-running",
            /// `not running`
            NotRunning = "not-running",
            /// `applied before`
            AppliedBefore = "applied-before",
            /// `saved, not applied`
            SavedNotApplied = "saved-not-applied",
            /// `A member is saved as a new revision, which you apply separately.`
            MemberSavedNewRevisionWhichYou = "member-saved-new-revision-which-you",
            /// `Add a member`
            AddAMember = "add-a-member",
            /// `Add FHIR endpoint`
            AddFhirEndpoint = "add-fhir-endpoint",
            /// `The exchange-format endpoints that serve the categories each one names.`
            ExchangeFormatEndpointsServeCategoriesEach = "exchange-format-endpoints-serve-categories-each",
            /// `FHIR endpoints`
            FhirEndpoints = "fhir-endpoints",
            /// `Add endpoint`
            AddEndpoint = "add-endpoint",
            /// `The ITS-REST endpoints the gateway sends each member's AQL to.`
            RestEndpointsGatewaySendsEachMember = "rest-endpoints-gateway-sends-each-member",
            /// `Query API endpoints`
            QueryApiEndpoints = "query-api-endpoints",
            /// `CDR product version`
            CdrProductVersion = "cdr-product-version",
            /// `CDR product`
            CdrProduct = "cdr-product",
            /// `A member's node_id is its identity: add a new member to change it.`
            MemberSNodeIdIdentityAdd = "member-s-node-id-identity-add",
            /// `Organisation name`
            OrganisationName = "organisation-name",
            /// `Organisation id`
            OrganisationId = "organisation-id",
            /// `Add profile`
            AddProfile = "add-profile",
            /// `Profiles`
            Profiles = "profiles",
            /// `The system the endpoint is told the ehr_id in, such as urn:oid:2.999.1.`
            SystemEndpointToldEhrIdSuch = "system-endpoint-told-ehr-id-such",
            /// `Identifier system of the ehr_id`
            IdentifierSystemEhrId = "identifier-system-ehr-id",
            /// `Base URL`
            BaseUrl = "base-url",
            /// `Endpoint id`
            EndpointId = "endpoint-id",
            /// `New FHIR endpoint`
            NewFhirEndpoint = "new-fhir-endpoint",
            /// `Categories`
            Categories = "categories",
            /// `Remove this profile`
            RemoveThisProfile = "remove-this-profile",
            /// `Profile`
            Profile = "profile",
            /// `Discharge reports`
            DischargeReports2 = "discharge-reports-2",
            /// `Laboratory reports`
            LaboratoryReports2 = "laboratory-reports-2",
            /// `Medical imaging`
            MedicalImaging2 = "medical-imaging-2",
            /// `Electronic dispensations`
            ElectronicDispensations2 = "electronic-dispensations-2",
            /// `Electronic prescriptions`
            ElectronicPrescriptions2 = "electronic-prescriptions-2",
            /// `Patient summaries`
            PatientSummaries2 = "patient-summaries-2",
            /// `New Query API endpoint`
            NewQueryApiEndpoint = "new-query-api-endpoint",
            /// `In service`
            InService = "in-service",
            /// `The console does not change this member.`
            ConsoleDoesNotChangeMember = "console-does-not-change-member",
            /// `The registry document provides a member with the same identity, which runs in its place.`
            RegistryDocumentProvidesMemberSameIdentity = "registry-document-provides-member-same-identity",
            /// `The newest revision removes this member; apply it from the card above.`
            NewestRevisionRemovesMemberApplyFrom = "newest-revision-removes-member-apply-from",
            /// `The care services directory provides this member: change it there.`
            CareServicesDirectoryProvidesMemberChange = "care-services-directory-provides-member-change",
            /// `The configuration file's registry document provides this member: change it there.`
            ConfigurationFileSRegistryDocumentProvides = "configuration-file-s-registry-document-provides",
            /// `Members and their owners`
            MembersAndTheirOwners = "members-and-their-owners",
            /// `Add member`
            AddMember = "add-member",
            /// `A member the registry document or a care services directory provides is read-only here.`
            MemberRegistryDocumentCareServicesDirectory = "member-registry-document-care-services-directory",
            /// `No member is configured yet.`
            NoMemberConfiguredYet = "no-member-configured-yet",
            /// `View`
            View = "view",
            /// `shadowed by the file`
            ShadowedByTheFile = "shadowed-by-the-file",
            /// `removed, not applied`
            RemovedNotApplied = "removed-not-applied",
            /// `Elsewhere, read-only`
            ElsewhereReadOnly = "elsewhere-read-only",
            /// `Store, immutable`
            StoreImmutable = "store-immutable",
            /// `Gateway, read-only`
            GatewayReadOnly = "gateway-read-only",
            /// `Directory, read-only`
            DirectoryReadOnly = "directory-read-only",
            /// `File, read-only`
            FileReadOnly = "file-read-only",
            /// `an author this console does not know`
            AuthorConsoleDoesNotKnow = "author-console-does-not-know",
            /// `the gateway, as no operator confirmed in time`
            GatewayNoOperatorConfirmedTime = "gateway-no-operator-confirmed-time",
            /// `a loopback peer under the development profile`
            LoopbackPeerUnderDevelopmentProfile = "loopback-peer-under-development-profile",
            /// `the configuration file`
            TheConfigurationFile = "the-configuration-file",
            /// `Import and export`
            ImportAndExport = "import-and-export",
            /// `a new revision`
            ANewRevision = "a-new-revision",
            /// `Setting`
            Setting = "setting",
            /// `Running value`
            RunningValue = "running-value",
            /// `It changes no setting.`
            ItChangesNoSetting = "it-changes-no-setting",
            /// `New value:`
            NewValue = "new-value",
            /// `Running value:`
            RunningValue2 = "running-value-2",
            /// `Saving does not apply it: apply it from the card at the top.`
            SavingDoesNotApplyApplyFrom = "saving-does-not-apply-apply-from",
            /// `The history records it with the revision.`
            HistoryRecordsRevision = "history-records-revision",
            /// `Breadcrumb`
            Breadcrumb = "breadcrumb",
            /// `The gateway stores it at once and every replica serves it.`
            GatewayStoresOnceEveryReplicaServes = "gateway-stores-once-every-replica-serves",
            /// `Store a new version`
            StoreANewVersion = "store-a-new-version",
            /// `Store`
            Store = "store",
            /// `Store this version? A stored version cannot be changed or removed afterwards.`
            StoreVersionStoredVersionCannotChanged = "store-version-stored-version-cannot-changed",
            /// `Store the version`
            StoreTheVersion = "store-the-version",
            /// `major.minor.patch, such as 1.0.0.`
            MajorMinorPatchSuch10 = "major-minor-patch-such-1-0",
            /// `A namespace and a name, such as org.example::vital-signs.`
            NamespaceNameSuchOrgExampleVital = "namespace-name-such-org-example-vital",
            /// `Qualified name`
            QualifiedName = "qualified-name",
            /// `Stored-query versions by name`
            StoredQueryVersionsName = "stored-query-versions-name",
            /// `No version is stored yet.`
            NoVersionStoredYet = "no-version-stored-yet",
            /// `The stored queries come from the read-only definition files: change the files.`
            StoredQueriesComeFromReadOnly = "stored-queries-come-from-read-only",
            /// `The gateway offers no stored-query registry.`
            GatewayOffersNoStoredQueryRegistry = "gateway-offers-no-stored-query-registry",
            /// `A stored version is immutable: a change is a new version, and none is edited or removed.`
            StoredVersionImmutableChangeNewVersion = "stored-version-immutable-change-new-version",
            /// `Saved and not applied. Read what it changes, then apply it.`
            SavedNotAppliedReadWhatChanges = "saved-not-applied-read-what-changes",
            /// `Apply`
            Apply = "apply",
            /// `Approve`
            Approve = "approve",
            /// `none yet`
            NoneYet = "none-yet",
            /// `Confirm`
            Confirm = "confirm",
            /// `, the gateway rolls it back.`
            GatewayRollsBack = "gateway-rolls-back",
            /// `It changed`
            ItChanged = "it-changed",
            /// . Unless an operator confirms it by
            UnlessOperatorConfirms = "unless-operator-confirms",
            /// `Confirm it once the gateway works with it, or roll it back.`
            ConfirmOnceGatewayWorksRollBack = "confirm-once-gateway-works-roll-back",
            /// `Roll back`
            RollBack = "roll-back",
            /// `No revision runs`
            NoRevisionRuns = "no-revision-runs",
            /// `The gateway's settings as revisions: each change is saved, then applied.`
            GatewaySSettingsRevisionsEachChange = "gateway-s-settings-revisions-each-change",
            ///  It still takes an approval before it applies.
            StillTakesApprovalBeforeApplies = "still-takes-approval-before-applies",
            ///  It may now be applied.
            MayNowApplied = "may-now-applied",
            ///  It stays running, and the gateway no longer rolls it back.
            StaysRunningGatewayNoLongerRolls = "stays-running-gateway-no-longer-rolls",
            /// `Choose Keep, Set a value, Read from a file or Clear.`
            ChooseKeepSetValueReadFrom = "choose-keep-set-value-read-from",
            /// `Name a secret this resource takes, such as oauth2.client_secret.`
            NameSecretResourceTakesSuchOauth2 = "name-secret-resource-takes-such-oauth2",
            /// `A setting's key is letters, digits, '_' and '-', its parts joined by '.'.`
            SettingSKeyLettersDigitsParts = "setting-s-key-letters-digits-parts",
            /// `The form names a setting this grant does not take; reload the page.`
            FormNamesSettingGrantDoesNot = "form-names-setting-grant-does-not",
            /// `The gateway's settings for this resource do not read as TOML.`
            GatewaySSettingsResourceDoNot = "gateway-s-settings-resource-do-not",
            /// `Mutual TLS alias hosts`
            MutualTlsAliasHosts = "mutual-tls-alias-hosts",
            /// `Trust roots file`
            TrustRootsFile = "trust-roots-file",
            /// `DPoP key file`
            DpopKeyFile = "dpop-key-file",
            /// `Key file`
            KeyFile = "key-file",
            /// `Key id`
            KeyId = "key-id",
            /// `DID`
            Did = "did",
            /// `Client id`
            ClientId = "client-id",
            /// `Scope`
            Scope = "scope",
            /// `Authorization server`
            AuthorizationServer = "authorization-server",
            /// `Audience`
            Audience = "audience",
            /// `Authorization details (JSON)`
            AuthorizationDetailsJson = "authorization-details-json",
            /// `Bind tokens to the TLS client certificate`
            BindTokensTlsClientCertificate = "bind-tokens-tls-client-certificate",
            /// `Previous client key file`
            PreviousClientKeyFile = "previous-client-key-file",
            /// `Client key file`
            ClientKeyFile = "client-key-file",
            /// `Self-signed TLS client certificate`
            SelfSignedTlsClientCertificate = "self-signed-tls-client-certificate",
            /// `TLS client certificate (tls_client_auth)`
            TlsClientCertificateTlsClientAuth = "tls-client-certificate-tls-client-auth",
            /// `Not set (private_key_jwt)`
            NotSetPrivateKeyJwt = "not-set-private-key-jwt",
            /// `Client authentication`
            ClientAuthentication = "client-authentication",
            /// `Grant`
            Grant = "grant",
            /// `Issuer`
            Issuer = "issuer",
            /// `The issuer`
            TheIssuer = "the-issuer",
            /// `The token endpoint`
            TheTokenEndpoint = "the-token-endpoint",
            /// `Not set`
            NotSet3 = "not-set-3",
            /// `Token endpoint`
            TokenEndpoint = "token-endpoint",
            /// `Token exchange`
            TokenExchange = "token-exchange",
            /// `Client credentials`
            ClientCredentials = "client-credentials",
            /// `Client secret, in the body`
            ClientSecretBody = "client-secret-body",
            /// `Client secret, Basic scheme`
            ClientSecretBasicScheme = "client-secret-basic-scheme",
            /// `User name`
            UserName = "user-name",
            /// `TLS client certificate and key (PEM)`
            TlsClientCertificateKeyPem = "tls-client-certificate-key-pem",
            /// `Client secret`
            ClientSecret = "client-secret",
            /// `Password`
            Password = "password",
            /// `Bearer token`
            /// `Nuts`
            Nuts = "nuts",
            /// `FAPI 2.0`
            Fapi20 = "fapi-2-0",
            /// `OAuth 2.0`
            Oauth20 = "oauth-2-0",
            /// `Basic authentication`
            BasicAuthentication = "basic-authentication",
            /// `TLS client certificate only`
            TlsClientCertificateOnly = "tls-client-certificate-only",
            /// `$count lines`
            Lines = "lines",
            /// Replica `$name` refused the export: `$status` (`$code`).
            ReplicaRefusedTheExport = "replica-refused-the-export",
            /// Replica `$name` could not be reached, or its answer read, for the export.
            ReplicaCouldNotReachedAnswerRead = "replica-could-not-reached-answer-read",
            /// attachment; filename="ferrofed-revision-`$id`.toml"
            AttachmentFilenameFerrofedRevisionToml = "attachment-filename-ferrofed-revision-toml",
            /// The organisation id is not `$id_form`.
            OrganisationIdNot = "organisation-id-not",
            /// The `node_id` is not `$id_form`.
            NodeIdNot = "node-id-not",
            /// The `endpoint_id` of `$kind` endpoint `$position` is not 1 to 64 letters, digits, '.',...
            EndpointIdEndpointNot164 = "endpoint-id-endpoint-not-1-64",
            /// The base URL of `$kind` endpoint `$position` is not an absolute http or https URL.
            BaseUrlEndpointNotAbsoluteHttp = "base-url-endpoint-not-absolute-http",
            /// FHIR endpoint `$position` names no identifier system for the `ehr_id`.
            FhirEndpointNamesNoIdentifierSystem = "fhir-endpoint-names-no-identifier-system",
            /// FHIR endpoint `$position` serves no category: choose at least one.
            FhirEndpointServesNoCategoryChoose = "fhir-endpoint-serves-no-category-choose",
            /// FHIR endpoint `$position` names no profile: add at least one.
            FhirEndpointNamesNoProfileAdd = "fhir-endpoint-names-no-profile-add",
            /// The `$what` is not a revision number.
            NotRevisionNumber = "not-revision-number",
            /// Name the `$what`.
            NameThe = "name-the",
            /// The TOML is larger than `$value0` bytes.
            TomlLargerThanBytes = "toml-larger-than-bytes",
            /// What it changes against the running revision `$running`:
            WhatChangesAgainstRunningRevision = "what-changes-against-running-revision",
            /// Replica `$replica` saved revision `$id`.
            ReplicaSavedRevision = "replica-saved-revision",
            /// , restoring the content of revision `$restored`
            RestoringContentRevision = "restoring-content-revision",
            /// Replica `$replica` applied revision `$id``$restored`.
            ReplicaAppliedRevision = "replica-applied-revision",
            /// Replica `$replica` stored `$name` version `$version`.
            ReplicaStoredVersion = "replica-stored-version",
            /// Replica `$replica` imported revision `$id` as a dry run.
            ReplicaImportedRevisionDryRun = "replica-imported-revision-dry-run",
            /// What it changes against the running revision `$from`:
            WhatChangesAgainstRunningRevision2 = "what-changes-against-running-revision-2",
            /// Value of `$key`
            ValueOf = "value-of",
            /// Remove `$named`
            Remove2 = "remove-2",
            /// Kept as the revision holds them: `$kept`.
            KeptRevisionHoldsThem = "kept-revision-holds-them",
            /// No binding section is called `$name`.
            NoBindingSectionCalled = "no-binding-section-called",
            /// The console holds no `$name` section.
            ConsoleHoldsNoSection = "console-holds-no-section",
            /// Edit `$value0`
            Edit2 = "edit-2",
            /// `$many settings`
            Settings2 = "settings-2",
            /// Remove `$shown`? It is saved as `$loaded` without it, which you apply separately.
            RemoveSavedWithoutWhichYouApply = "remove-saved-without-which-you-apply",
            /// Set, read from `$reference`.
            SetReadFrom2 = "set-read-from-2",
            /// The console holds no credentials of `$endpoint`.
            ConsoleHoldsNoCredentials = "console-holds-no-credentials",
            /// Edit the credentials of `$endpoint`
            EditTheCredentialsOf = "edit-the-credentials-of",
            /// rollback to `$restored`
            RollbackTo = "rollback-to",
            ///  (revision `$id`)
            Revision = "revision",
            /// Revision `$id`
            Revision2 = "revision-2",
            /// Export revision `$id` as TOML
            ExportRevisionAsToml = "export-revision-as-toml",
            /// The `$history` newest of `$total` revisions, newest first.
            NewestRevisionsNewestFirst = "newest-revisions-newest-first",
            /// Revision `$other`
            Revision3 = "revision-3",
            /// Revision `$other` against the running revision `$running`
            RevisionAgainstRunningRevision = "revision-against-running-revision",
            /// It is saved as `$loaded` and not applied; the outcome shows what it changes.
            SavedNotAppliedOutcomeShowsWhat = "saved-not-applied-outcome-shows-what",
            /// Remove member `$node`? It is saved as `$loaded` without it, which you apply separately.
            RemoveMemberSavedWithoutWhichYou = "remove-member-saved-without-which-you",
            /// No member has the `node_id` `$node`.
            NoMemberHasNodeId = "no-member-has-node-id",
            /// Query API endpoint `$endpoint`
            QueryApiEndpoint = "query-api-endpoint",
            /// Remove `$named`
            Remove3 = "remove-3",
            /// FHIR endpoint `$endpoint`
            FhirEndpoint = "fhir-endpoint",
            /// Remove `$named`
            Remove4 = "remove-4",
            /// Edit member `$member`
            EditMember = "edit-member",
            /// revision `$next`
            Revision4 = "revision-4",
            /// Replica `$name` serves no configuration revisions: its configuration is not managed...
            ReplicaServesNoConfigurationRevisionsConfiguration = "replica-serves-no-configuration-revisions-configuration",
            /// operator `$subject`
            Operator2 = "operator-2",
            ///  Some of its settings take effect only at a restart: `$mark`.
            SomeSettingsTakeEffectOnlyRestart = "some-settings-take-effect-only-restart",
            /// Revision `$revision` runs, applied by `$value1` at `$value2`.`$outcome`
            RevisionRunsApplied = "revision-runs-applied",
            /// Save as `$loaded`
            SaveAs = "save-as",
            /// Revision `$from` (running)
            RevisionRunning = "revision-running",
            /// Revision `$to`
            Revision5 = "revision-5",
            /// What revision `$to` changes, by key
            WhatRevisionChangesKey = "what-revision-changes-key",
            /// Revision `$running` running
            RevisionRunning2 = "revision-running-2",
            /// Revision `$head` saved, not applied
            RevisionSavedNotApplied = "revision-saved-not-applied",
            /// Roll back revision `$running`? The content of revision `$previous` is saved and...
            RollBackRevisionContentRevisionSaved = "roll-back-revision-content-revision-saved",
            /// Revision `$revision` awaits your confirmation
            RevisionAwaitsYourConfirmation = "revision-awaits-your-confirmation",
            /// Confirm revision `$revision`? The gateway keeps it and stops the countdown.
            ConfirmRevisionGatewayKeepsStopsCountdown = "confirm-revision-gateway-keeps-stops-countdown",
            /// `$value0 at $value1`
            At2 = "at-2",
            /// Once applied, it awaits a confirmation: it changes `$review`.
            OnceAppliedAwaitsConfirmationChanges = "once-applied-awaits-confirmation-changes",
            /// It changes `$review`, which take the approval of an operator other than its author,...
            ChangesWhichTakeApprovalOperatorOther = "changes-which-take-approval-operator-other",
            /// Approve revision `$revision`
            ApproveRevision = "approve-revision",
            /// Approve revision `$revision` as an operator other than its author? The approval names you.
            ApproveRevisionOperatorOtherThanAuthor = "approve-revision-operator-other-than-author",
            /// `$many settings`
            Settings3 = "settings-3",
            /// Revision `$head` changes `$count`
            RevisionChanges = "revision-changes",
            /// Revision `$head` is saved and not applied
            RevisionSavedNotApplied2 = "revision-saved-not-applied-2",
            /// Apply revision `$head`
            ApplyRevision = "apply-revision",
            /// Apply revision `$head` to the running gateway? Every replica follows it.
            ApplyRevisionRunningGatewayEveryReplica = "apply-revision-running-gateway-every-replica",
            /// Replica `$replica` recorded the confirmation of revision `$revision`.
            ReplicaRecordedConfirmationRevision = "replica-recorded-confirmation-revision",
            /// Replica `$replica` recorded the approval of revision `$revision`.
            ReplicaRecordedApprovalRevision = "replica-recorded-approval-revision",
            /// settings = `$text`
            Settings4 = "settings-4",
            /// The setting `$key` is named twice.
            SettingNamedTwice = "setting-named-twice",
            /// `$key is a secret: set it under Secrets.`
            SecretSetUnderSecrets = "secret-set-under-secrets",
            /// The value of `$key` does not read as the setting's type.
            ValueDoesNotReadSettingS = "value-does-not-read-setting-s",
            /// Type the value of `$key`, or choose Keep.
            TypeValueChooseKeep = "type-value-choose-keep",
            /// Give the absolute path of the file `$key` is read from.
            GiveAbsolutePathFileReadFrom = "give-absolute-path-file-read-from",
            /// The file `$key` is read from is not an absolute path.
            FileReadFromNotAbsolutePath = "file-read-from-not-absolute-path",
            /// `Client assertion audience`
            ClientAssertionAudience = "client-assertion-audience",
            /// `Signed client assertion (private_key_jwt)`
            SignedClientAssertionPrivateKeyJwt = "signed-client-assertion-private-key-jwt",
            /// A resource that runs.
            StateRunning = "state-running",
            /// A state the console does not know.
            StateUnknown = "state-unknown",
            /// The label of the member's node identifier.
            NodeIdLabel = "node-id-label",
            /// The hint that names the configuration key `$key` a field writes.
            ConfigurationKey = "configuration-key",
            /// The object a stored-query action reports on.
            StoredQueryStore = "stored-query-store",
        }
    };
}
