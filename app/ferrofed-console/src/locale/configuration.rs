// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The messages of `locales/en/configuration.ftl`.

messages! {
    /// `a new revision`
    ANewRevision = "a-new-revision",
    /// `Absolute path of the file it is read from`
    AbsolutePathFileReadFrom = "absolute-path-file-read-from",
    /// `An absolute path on the gateway's host.`
    AbsolutePathGatewaySHost = "absolute-path-gateway-s-host",
    /// `Actions`
    Actions = "actions",
    /// `Actions for { $named }`
    ActionsFor = "actions-for",
    /// `Add a binding section`
    AddABindingSection = "add-a-binding-section",
    /// `Add a member`
    AddAMember = "add-a-member",
    /// `Add binding section`
    AddBindingSection = "add-binding-section",
    /// `Add credentials`
    AddCredentials = "add-credentials",
    /// `Add endpoint`
    AddEndpoint = "add-endpoint",
    /// `Add FHIR endpoint`
    AddFhirEndpoint = "add-fhir-endpoint",
    /// `Add member`
    AddMember = "add-member",
    /// `Add onward credentials`
    AddOnwardCredentials = "add-onward-credentials",
    /// `Add profile`
    AddProfile = "add-profile",
    /// `Add secret`
    AddSecret = "add-secret",
    /// `Add setting`
    AddSetting = "add-setting",
    /// `Add value`
    AddValue = "add-value",
    /// `applied before`
    AppliedBefore = "applied-before",
    /// `Apply`
    Apply = "apply",
    /// `Apply revision …`
    ApplyRevision = "apply-revision",
    /// `Apply revision … to the running gateway? Every replica follows it.`
    ApplyRevisionRunningGatewayEveryReplica = "apply-revision-running-gateway-every-replica",
    /// `Approve`
    Approve = "approve",
    /// `Approve revision …`
    ApproveRevision = "approve-revision",
    /// `Approve revision … as an operator other than its author? The …`
    ApproveRevisionOperatorOtherThanAuthor = "approve-revision-operator-other-than-author",
    /// `AQL`
    Aql = "aql",
    /// `… at …`
    At2 = "at-2",
    /// `attachment; filename="ferrofed-revision-….toml"`
    AttachmentFilenameFerrofedRevisionToml = "attachment-filename-ferrofed-revision-toml",
    /// `Audience`
    Audience = "audience",
    /// `an author this console does not know`
    AuthorConsoleDoesNotKnow = "author-console-does-not-know",
    /// `Authorization details (JSON)`
    AuthorizationDetailsJson = "authorization-details-json",
    /// `Authorization server`
    AuthorizationServer = "authorization-server",
    /// `…It awaits your confirmation: confirm it in the card at the top, …`
    AwaitsYourConfirmationConfirmCardTop = "awaits-your-confirmation-confirm-card-top",
    /// `base revision`
    BaseRevision = "base-revision",
    /// `Base URL`
    BaseUrl = "base-url",
    /// `The base URL of … endpoint … is not an absolute http or https URL.`
    BaseUrlEndpointNotAbsoluteHttp = "base-url-endpoint-not-absolute-http",
    /// `Basic authentication`
    BasicAuthentication = "basic-authentication",
    /// `Bind tokens to the TLS client certificate`
    BindTokensTlsClientCertificate = "bind-tokens-tls-client-certificate",
    /// `Binding section`
    BindingSection = "binding-section",
    /// `Breadcrumb`
    Breadcrumb = "breadcrumb",
    /// `The care services directory provides this member: change it there.`
    CareServicesDirectoryProvidesMemberChange = "care-services-directory-provides-member-change",
    /// `Categories`
    Categories = "categories",
    /// `CDR product`
    CdrProduct = "cdr-product",
    /// `CDR product version`
    CdrProductVersion = "cdr-product-version",
    /// `It changes …, which take the approval of an operator other than …`
    ChangesWhichTakeApprovalOperatorOther = "changes-which-take-approval-operator-other",
    /// `Choose the console member to remove.`
    ChooseConsoleMemberRemove = "choose-console-member-remove",
    /// `Choose the credential or binding section to remove.`
    ChooseCredentialBindingSectionRemove = "choose-credential-binding-section-remove",
    /// `Choose the endpoint of the credentials.`
    ChooseEndpointCredentials = "choose-endpoint-credentials",
    /// `Choose how the gateway reaches the endpoint.`
    ChooseHowGatewayReachesEndpoint = "choose-how-gateway-reaches-endpoint",
    /// `Choose Keep, Set a value, Read from a file or Clear.`
    ChooseKeepSetValueReadFrom = "choose-keep-set-value-read-from",
    /// `Choose the binding section.`
    ChooseTheBindingSection = "choose-the-binding-section",
    /// `Clear`
    Clear = "clear",
    /// `Client assertion audience`
    ClientAssertionAudience = "client-assertion-audience",
    /// `Client authentication`
    ClientAuthentication = "client-authentication",
    /// `Client credentials`
    ClientCredentials = "client-credentials",
    /// `Client id`
    ClientId = "client-id",
    /// `Client key file`
    ClientKeyFile = "client-key-file",
    /// `Client secret`
    ClientSecret = "client-secret",
    /// `Client secret, Basic scheme`
    ClientSecretBasicScheme = "client-secret-basic-scheme",
    /// `Client secret, in the body`
    ClientSecretBody = "client-secret-body",
    /// `Close the comparison`
    CloseTheComparison = "close-the-comparison",
    /// `Compare with running`
    CompareWithRunning = "compare-with-running",
    /// `The profile, the client and admin listeners, client …`
    ConfigHowCore = "config-how-core",
    /// `The bootstrap core: always in ferrofed.toml`
    ConfigHowCoreTitle = "config-how-core-title",
    /// `Revisions are managed on this gateway (config.ui = …`
    ConfigHowLead = "config-how-lead",
    /// `To stop managing revisions, set config.ui = "off" and …`
    ConfigHowOff = "config-how-off",
    /// `A member the file or a care services directory provides, …`
    ConfigHowOwners = "config-how-owners",
    /// `Members, stored queries, onward credentials and binding …`
    ConfigHowRest = "config-how-rest",
    /// `Everything else: a revision`
    ConfigHowRestTitle = "config-how-rest-title",
    /// `How configuration works`
    ConfigHowTitle = "config-how-title",
    /// `This console is configured with no admin listener: it reads the …`
    ConfigNoAdminBody = "config-no-admin-body",
    /// `Name a replica's admin listener in the [[admin]] section of the …`
    ConfigNoAdminHow = "config-no-admin-how",
    /// `Configuration`
    Configuration = "configuration",
    /// `Configuration changes`
    ConfigurationChanges = "configuration-changes",
    /// `The configuration file's registry document provides this member: …`
    ConfigurationFileSRegistryDocumentProvides = "configuration-file-s-registry-document-provides",
    /// `Configuration key: …`
    ConfigurationKey = "configuration-key",
    /// `The configuration less its bootstrap core, each secret as its file …`
    ConfigurationLessBootstrapCoreEachSecret = "configuration-less-bootstrap-core-each-secret",
    /// `Confirm`
    Confirm = "confirm",
    /// `Confirm the change in its confirmation step to run it.`
    ConfirmChangeConfirmationStepRun = "confirm-change-confirmation-step-run",
    /// `Confirm it once the gateway works with it, or roll it back.`
    ConfirmOnceGatewayWorksRollBack = "confirm-once-gateway-works-roll-back",
    /// `Confirm revision …? The gateway keeps it and stops the countdown.`
    ConfirmRevisionGatewayKeepsStopsCountdown = "confirm-revision-gateway-keeps-stops-countdown",
    /// `Console`
    Console = "console",
    /// `This console is configured with no admin listener, so it shows no …`
    ConsoleConfiguredNoAdminListenerSo2 = "console-configured-no-admin-listener-so-2",
    /// `This console is configured with no admin listener, so it exports …`
    ConsoleConfiguredNoAdminListenerSo3 = "console-configured-no-admin-listener-so-3",
    /// `The console does not change this member.`
    ConsoleDoesNotChangeMember = "console-does-not-change-member",
    /// `The console holds no credentials of ….`
    ConsoleHoldsNoCredentials = "console-holds-no-credentials",
    /// `The console holds no … section.`
    ConsoleHoldsNoSection = "console-holds-no-section",
    /// `A credential or a binding section the file provides is read-only …`
    CredentialBindingSectionFileProvidesRead = "credential-binding-section-file-provides-read",
    /// `Credentials`
    Credentials = "credentials",
    /// `Credentials and bindings`
    CredentialsAndBindings = "credentials-and-bindings",
    /// `Credentials and binding sections and their owners`
    CredentialsBindingSectionsTheirOwners = "credentials-binding-sections-their-owners",
    /// `DID`
    Did = "did",
    /// `Directory, read-only`
    DirectoryReadOnly = "directory-read-only",
    /// `Discharge reports`
    DischargeReports2 = "discharge-reports-2",
    /// `Download as TOML`
    DownloadAsToml = "download-as-toml",
    /// `DPoP key file`
    DpopKeyFile = "dpop-key-file",
    /// `Each setting is its own field; a number or true or false is read …`
    EachSettingOwnFieldNumberTrue = "each-setting-own-field-number-true",
    /// `Edit`
    Edit = "edit",
    /// `Edit …`
    Edit2 = "edit-2",
    /// `Edit member …`
    EditMember = "edit-member",
    /// `Edit the credentials of …`
    EditTheCredentialsOf = "edit-the-credentials-of",
    /// `Electronic dispensations`
    ElectronicDispensations2 = "electronic-dispensations-2",
    /// `Electronic prescriptions`
    ElectronicPrescriptions2 = "electronic-prescriptions-2",
    /// `Elsewhere, read-only`
    ElsewhereReadOnly = "elsewhere-read-only",
    /// `Endpoint`
    Endpoint = "endpoint",
    /// `Endpoint…`
    Endpoint2 = "endpoint-2",
    /// `Endpoint and grant`
    EndpointAndGrant = "endpoint-and-grant",
    /// `Endpoint id`
    EndpointId = "endpoint-id",
    /// `The endpoint_id of … endpoint … is not 1 to 64 letters, digits, …`
    EndpointIdEndpointNot164 = "endpoint-id-endpoint-not-1-64",
    /// `The endpoint_id is not 1 to 64 letters, digits, '.', '-' or '_'.`
    EndpointIdNot164Letters = "endpoint-id-not-1-64-letters",
    /// `Endpoints`
    Endpoints = "endpoints",
    /// `The endpoints the members declare that hold no credentials yet.`
    EndpointsMembersDeclareHoldNoCredentials = "endpoints-members-declare-hold-no-credentials",
    /// `Every revision, who made it and why; each one downloads as TOML.`
    EveryRevisionWhoMadeWhyEach = "every-revision-who-made-why-each",
    /// `The exchange-format endpoints that serve the categories each one …`
    ExchangeFormatEndpointsServeCategoriesEach = "exchange-format-endpoints-serve-categories-each",
    /// `Export`
    Export = "export",
    /// `Export revision … as TOML`
    ExportRevisionAsToml = "export-revision-as-toml",
    /// `FAPI 2.0`
    Fapi20 = "fapi-2-0",
    /// `FHIR`
    Fhir = "fhir",
    /// `FHIR endpoint …`
    FhirEndpoint = "fhir-endpoint",
    /// `FHIR endpoint … names no identifier system for the ehr_id.`
    FhirEndpointNamesNoIdentifierSystem = "fhir-endpoint-names-no-identifier-system",
    /// `FHIR endpoint … names no profile: add at least one.`
    FhirEndpointNamesNoProfileAdd = "fhir-endpoint-names-no-profile-add",
    /// `FHIR endpoint … serves no category: choose at least one.`
    FhirEndpointServesNoCategoryChoose = "fhir-endpoint-serves-no-category-choose",
    /// `FHIR endpoints`
    FhirEndpoints = "fhir-endpoints",
    /// `The file … is read from is not an absolute path.`
    FileReadFromNotAbsolutePath = "file-read-from-not-absolute-path",
    /// `File, read-only`
    FileReadOnly = "file-read-only",
    /// `The form names no action this page offers.`
    FormNamesNoActionPageOffers = "form-names-no-action-page-offers",
    /// `The form names a setting this grant does not take; reload the page.`
    FormNamesSettingGrantDoesNot = "form-names-setting-grant-does-not",
    /// `The form's endpoint rows do not read; reload the page.`
    FormSEndpointRowsDoNot = "form-s-endpoint-rows-do-not",
    /// `the gateway, as no operator confirmed in time`
    GatewayNoOperatorConfirmedTime = "gateway-no-operator-confirmed-time",
    /// `The gateway offers no stored-query registry.`
    GatewayOffersNoStoredQueryRegistry = "gateway-offers-no-stored-query-registry",
    /// `Gateway, read-only`
    GatewayReadOnly = "gateway-read-only",
    /// `, the gateway rolls it back.`
    GatewayRollsBack = "gateway-rolls-back",
    /// `The gateway's settings for this resource do not read as TOML.`
    GatewaySSettingsResourceDoNot = "gateway-s-settings-resource-do-not",
    /// `The gateway's settings as revisions: each change is saved, then …`
    GatewaySSettingsRevisionsEachChange = "gateway-s-settings-revisions-each-change",
    /// `The gateway stores it at once and every replica serves it.`
    GatewayStoresOnceEveryReplicaServes = "gateway-stores-once-every-replica-serves",
    /// `Give the absolute path of the file … is read from.`
    GiveAbsolutePathFileReadFrom = "give-absolute-path-file-read-from",
    /// `Give the AQL to store.`
    GiveAqlStore = "give-aql-store",
    /// `Give at least one Query API endpoint.`
    GiveLeastOneQueryApiEndpoint = "give-least-one-query-api-endpoint",
    /// `Give the reason for the change.`
    GiveReasonChange = "give-reason-change",
    /// `Grant`
    Grant = "grant",
    /// `Grant settings`
    GrantSettings = "grant-settings",
    /// `Grant type`
    GrantType = "grant-type",
    /// `History`
    History = "history",
    /// `None of the newest revisions has an origin you chose.`
    HistoryNoneOfOrigin = "history-none-of-origin",
    /// `Origin`
    HistoryOrigin = "history-origin",
    /// `Show the revisions by origin`
    HistoryOriginTitle = "history-origin-title",
    /// `The history records it with the revision.`
    HistoryRecordsRevision = "history-records-revision",
    /// `How the gateway reaches the endpoint; the form shows that grant's …`
    HowGatewayReachesEndpointFormShows = "how-gateway-reaches-endpoint-form-shows",
    /// `Identifier system of the ehr_id`
    IdentifierSystemEhrId = "identifier-system-ehr-id",
    /// `Import`
    Import = "import",
    /// `Import and export`
    ImportAndExport = "import-and-export",
    /// `Import as a dry-run revision`
    ImportDryRunRevision = "import-dry-run-revision",
    /// `In service`
    InService = "in-service",
    /// `Issuer`
    Issuer = "issuer",
    /// `It changed…`
    ItChanged = "it-changed",
    /// `It changes no setting.`
    ItChangesNoSetting = "it-changes-no-setting",
    /// `Keep`
    Keep = "keep",
    /// `Kept as the revision holds them: ….`
    KeptRevisionHoldsThem = "kept-revision-holds-them",
    /// `Key`
    Key = "key",
    /// `Key file`
    KeyFile = "key-file",
    /// `Key id`
    KeyId = "key-id",
    /// `Laboratory reports`
    LaboratoryReports2 = "laboratory-reports-2",
    /// `Learn more`
    LearnMore = "learn-more",
    /// `Leave unset`
    LeaveUnset = "leave-unset",
    /// `Loading the configuration.`
    LoadingTheConfiguration = "loading-the-configuration",
    /// `a loopback peer under the development profile`
    LoopbackPeerUnderDevelopmentProfile = "loopback-peer-under-development-profile",
    /// `major.minor.patch, such as 1.0.0.`
    MajorMinorPatchSuch10 = "major-minor-patch-such-1-0",
    /// `…It may now be applied.`
    MayNowApplied = "may-now-applied",
    /// `Medical imaging`
    MedicalImaging2 = "medical-imaging-2",
    /// `A member with this node_id exists: change it with its own edit form.`
    MemberNodeIdExistsChangeOwn = "member-node-id-exists-change-own",
    /// `A member the registry document or a care services directory …`
    MemberRegistryDocumentCareServicesDirectory = "member-registry-document-care-services-directory",
    /// `A member's node_id is its identity: add a new member to change it.`
    MemberSNodeIdIdentityAdd = "member-s-node-id-identity-add",
    /// `A member is saved as a new revision, which you apply separately.`
    MemberSavedNewRevisionWhichYou = "member-saved-new-revision-which-you",
    /// `Members`
    Members = "members",
    /// `Members and their owners`
    MembersAndTheirOwners = "members-and-their-owners",
    /// `Mutual TLS alias hosts`
    MutualTlsAliasHosts = "mutual-tls-alias-hosts",
    /// `1 to 64 letters, digits, '.', '-' or '_', starting with a letter …`
    N164LettersDigitsStartingLetter = "n-1-64-letters-digits-starting-letter",
    /// `1 setting`
    N1Setting = "n-1-setting",
    /// `Name`
    Name = "name",
    /// `Name a secret this resource takes, such as oauth2.client_secret.`
    NameSecretResourceTakesSuchOauth2 = "name-secret-resource-takes-such-oauth2",
    /// `Name the ….`
    NameThe = "name-the",
    /// `Name the stored query.`
    NameTheStoredQuery = "name-the-stored-query",
    /// `Name the version to store.`
    NameVersionStore = "name-version-store",
    /// `A namespace and a name, such as org.example::vital-signs.`
    NamespaceNameSuchOrgExampleVital = "namespace-name-such-org-example-vital",
    /// `New FHIR endpoint`
    NewFhirEndpoint = "new-fhir-endpoint",
    /// `New Query API endpoint`
    NewQueryApiEndpoint = "new-query-api-endpoint",
    /// `New secret`
    NewSecret = "new-secret",
    /// `New setting`
    NewSetting = "new-setting",
    /// `New value:…`
    NewValue = "new-value",
    /// `The newest revision removes this member; apply it from the card above.`
    NewestRevisionRemovesMemberApplyFrom = "newest-revision-removes-member-apply-from",
    /// `The … newest of … revisions, newest first.`
    NewestRevisionsNewestFirst = "newest-revisions-newest-first",
    /// `No binding section is called ….`
    NoBindingSectionCalled = "no-binding-section-called",
    /// `no code`
    NoCode = "no-code",
    /// `No FHIR endpoint yet.`
    NoFhirEndpointYet = "no-fhir-endpoint-yet",
    /// `No member is configured yet.`
    NoMemberConfiguredYet = "no-member-configured-yet",
    /// `No member has the node_id ….`
    NoMemberHasNodeId = "no-member-has-node-id",
    /// `No new secret.`
    NoNewSecretYet = "no-new-secret-yet",
    /// `No onward credential or binding section is set.`
    NoOnwardCredentialBindingSectionSet = "no-onward-credential-binding-section-set",
    /// `No profile yet.`
    NoProfileYet = "no-profile-yet",
    /// `No Query API endpoint yet.`
    NoQueryEndpointYet = "no-query-endpoint-yet",
    /// `No revision runs`
    NoRevisionRuns = "no-revision-runs",
    /// `No revision runs, so none downloads.`
    NoRevisionRunsSoNoneDownloads = "no-revision-runs-so-none-downloads",
    /// `No setting yet.`
    NoSettingYet = "no-setting-yet",
    /// `no settings`
    NoSettings = "no-settings",
    /// `No value yet.`
    NoValueYet = "no-value-yet",
    /// `No version is stored yet.`
    NoVersionStoredYet = "no-version-stored-yet",
    /// `Node`
    Node = "node",
    /// `Node id`
    NodeIdLabel = "node-id-label",
    /// `The node_id is not ….`
    NodeIdNot = "node-id-not",
    /// `none yet`
    NoneYet = "none-yet",
    /// `…It is not applied: apply it from the card at the top once you …`
    NotAppliedApplyFromCardTop = "not-applied-apply-from-card-top",
    /// `not readable`
    NotReadable = "not-readable",
    /// `The … is not a revision number.`
    NotRevisionNumber = "not-revision-number",
    /// `not running`
    NotRunning = "not-running",
    /// `Not set.`
    NotSet = "not-set",
    /// `not set`
    NotSet2 = "not-set-2",
    /// `Not set`
    NotSet3 = "not-set-3",
    /// `Not set (private_key_jwt)`
    NotSetPrivateKeyJwt = "not-set-private-key-jwt",
    /// `Nuts`
    Nuts = "nuts",
    /// `OAuth 2.0`
    Oauth20 = "oauth-2-0",
    /// `Once applied, it awaits a confirmation: it changes ….`
    OnceAppliedAwaitsConfirmationChanges = "once-applied-awaits-confirmation-changes",
    /// `operator …`
    Operator2 = "operator-2",
    /// `Organisation`
    Organisation = "organisation",
    /// `Organisation id`
    OrganisationId = "organisation-id",
    /// `The organisation id is not ….`
    OrganisationIdNot = "organisation-id-not",
    /// `Organisation name`
    OrganisationName = "organisation-name",
    /// `File`
    OriginFile = "origin-file",
    /// `Import`
    OriginImport = "origin-import",
    /// `Operator`
    OriginOperator = "origin-operator",
    /// `Other`
    OriginOther = "origin-other",
    /// `Rollback`
    OriginRollback = "origin-rollback",
    /// `Owner`
    Owner = "owner",
    /// `Its parts joined by dots, such as oauth2.scope.`
    PartsJoinedDotsSuchOauth2Scope = "parts-joined-dots-such-oauth2-scope",
    /// `Password`
    Password = "password",
    /// `Paste the TOML an export wrote, or one kept in version control.`
    PasteTomlExportWroteOneKept = "paste-toml-export-wrote-one-kept",
    /// `Paste the TOML to import.`
    PasteTomlImport = "paste-toml-import",
    /// `This path names no part of the configuration page.`
    PathNamesNoPartConfigurationPage = "path-names-no-part-configuration-page",
    /// `The path names no revision number.`
    PathNamesNoRevisionNumber = "path-names-no-revision-number",
    /// `Patient summaries`
    PatientSummaries2 = "patient-summaries-2",
    /// `The preview of its changes could not be read; reload the page to …`
    PreviewChangesCouldNotReadReload = "preview-changes-could-not-read-reload",
    /// `Previous client key file`
    PreviousClientKeyFile = "previous-client-key-file",
    /// `Profile`
    Profile = "profile",
    /// `Profiles`
    Profiles = "profiles",
    /// `Qualified name`
    QualifiedName = "qualified-name",
    /// `Query API`
    QueryApi = "query-api",
    /// `Query API endpoint …`
    QueryApiEndpoint = "query-api-endpoint",
    /// `Query API endpoints`
    QueryApiEndpoints = "query-api-endpoints",
    /// `Read from a file`
    ReadFromAFile = "read-from-a-file",
    /// `Reason`
    Reason = "reason",
    /// `The reason is longer than 1000 characters.`
    ReasonLongerThan1000Characters = "reason-longer-than-1000-characters",
    /// `The registry document provides a member with the same identity, …`
    RegistryDocumentProvidesMemberSameIdentity = "registry-document-provides-member-same-identity",
    /// `It is saved as … without it, which you apply separately.`
    RemovalSavedWithoutIt = "removal-saved-without-it",
    /// `Remove`
    Remove = "remove",
    /// `Remove …`
    Remove2 = "remove-2",
    /// `Remove …`
    Remove3 = "remove-3",
    /// `Remove …`
    Remove4 = "remove-4",
    /// `Remove member …? It is saved as … without it, which you apply …`
    RemoveMemberSavedWithoutWhichYou = "remove-member-saved-without-which-you",
    /// `Remove member …?`
    RemoveMemberTitle = "remove-member-title",
    /// `Remove …? It is saved as … without it, which you apply separately.`
    RemoveSavedWithoutWhichYouApply = "remove-saved-without-which-you-apply",
    /// `Remove …?`
    RemoveSectionTitle = "remove-section-title",
    /// `Remove this new secret`
    RemoveThisNewSecret = "remove-this-new-secret",
    /// `Remove this profile`
    RemoveThisProfile = "remove-this-profile",
    /// `Remove this value`
    RemoveThisValue = "remove-this-value",
    /// `Show the value while held`
    ShowSecretWhileHeld = "show-secret-while-held",
    /// `removing`
    StateRemovingWord = "state-removing-word",
    /// `Replica …: …`
    Replica2 = "replica-2",
    /// `Replica …: ….`
    Replica3 = "replica-3",
    /// `Replica …: its admin listener gave no answer, so whether the … ran …`
    ReplicaAdminListenerGaveNoAnswer = "replica-admin-listener-gave-no-answer",
    /// `Replica … applied revision …….`
    ReplicaAppliedRevision = "replica-applied-revision",
    /// `Replica … could not be reached, or its answer read, for the export.`
    ReplicaCouldNotReachedAnswerRead = "replica-could-not-reached-answer-read",
    /// `Replica … did not accept your sign-in for the …: 401 (…).…`
    ReplicaDidNotAcceptYourSign = "replica-did-not-accept-your-sign",
    /// `Replica … imported revision … as a dry run.`
    ReplicaImportedRevisionDryRun = "replica-imported-revision-dry-run",
    /// `Replica … recorded the approval of revision ….`
    ReplicaRecordedApprovalRevision = "replica-recorded-approval-revision",
    /// `Replica … recorded the confirmation of revision ….`
    ReplicaRecordedConfirmationRevision = "replica-recorded-confirmation-revision",
    /// `Replica … refused the …: … (…).…`
    ReplicaRefusedThe = "replica-refused-the",
    /// `Replica … refused the export: … (…).`
    ReplicaRefusedTheExport = "replica-refused-the-export",
    /// `Replica … saved revision ….`
    ReplicaSavedRevision = "replica-saved-revision",
    /// `Replica … serves no configuration revisions: its configuration is …`
    ReplicaServesNoConfigurationRevisionsConfiguration = "replica-serves-no-configuration-revisions-configuration",
    /// `Replica … stored … version ….`
    ReplicaStoredVersion = "replica-stored-version",
    /// `Resource`
    Resource = "resource",
    /// `The ITS-REST endpoints the gateway sends each member's AQL to.`
    RestEndpointsGatewaySendsEachMember = "rest-endpoints-gateway-sends-each-member",
    /// `, restoring the content of revision …`
    RestoringContentRevision = "restoring-content-revision",
    /// `…(revision …)`
    Revision = "revision",
    /// `Revision …`
    Revision2 = "revision-2",
    /// `Revision …`
    Revision3 = "revision-3",
    /// `revision …`
    Revision4 = "revision-4",
    /// `Revision …`
    Revision5 = "revision-5",
    /// `Revision … against the running revision …`
    RevisionAgainstRunningRevision = "revision-against-running-revision",
    /// `Revision … awaits your confirmation`
    RevisionAwaitsYourConfirmation = "revision-awaits-your-confirmation",
    /// `Revision … changes …`
    RevisionChanges = "revision-changes",
    /// `Revision … (running)`
    RevisionRunning = "revision-running",
    /// `Revision … running`
    RevisionRunning2 = "revision-running-2",
    /// `Revision … runs, applied by … at ….…`
    RevisionRunsApplied = "revision-runs-applied",
    /// `Revision … saved, not applied`
    RevisionSavedNotApplied = "revision-saved-not-applied",
    /// `Revision … is saved and not applied`
    RevisionSavedNotApplied2 = "revision-saved-not-applied-2",
    /// `revision to apply`
    RevisionToApply = "revision-to-apply",
    /// `revision to approve`
    RevisionToApprove = "revision-to-approve",
    /// `revision to confirm`
    RevisionToConfirm = "revision-to-confirm",
    /// `Roll back`
    RollBack = "roll-back",
    /// `Roll back revision …? The content of revision … is saved and …`
    RollBackRevisionContentRevisionSaved = "roll-back-revision-content-revision-saved",
    /// `rollback to …`
    RollbackTo = "rollback-to",
    /// `running revision to roll back`
    RunningRevisionRollBack = "running-revision-roll-back",
    /// `Running value`
    RunningValue = "running-value",
    /// `Running value:…`
    RunningValue2 = "running-value-2",
    /// `Save as …`
    SaveAs = "save-as",
    /// `Save the removal`
    SaveTheRemoval = "save-the-removal",
    /// `pending`
    StatePendingWord = "state-pending-word",
    /// `It is saved as … and not applied; the outcome shows what it changes.`
    SavedNotAppliedOutcomeShowsWhat = "saved-not-applied-outcome-shows-what",
    /// `Saved and not applied. Read what it changes, then apply it.`
    SavedNotAppliedReadWhatChanges = "saved-not-applied-read-what-changes",
    /// `Saved as a revision; once applied it awaits a confirmation, or the …`
    SavedRevisionOnceAppliedAwaitsConfirmation = "saved-revision-once-applied-awaits-confirmation",
    /// `Saving does not apply it: apply it from the card at the top.`
    SavingDoesNotApplyApplyFrom = "saving-does-not-apply-apply-from",
    /// `Scope`
    Scope = "scope",
    /// `Secret key`
    SecretKey = "secret-key",
    /// `… is a secret: set it under Secrets.`
    SecretSetUnderSecrets = "secret-set-under-secrets",
    /// `Secrets`
    Secrets = "secrets",
    /// `Section`
    Section = "section",
    /// `Self-signed TLS client certificate`
    SelfSignedTlsClientCertificate = "self-signed-tls-client-certificate",
    /// `Sending the change.`
    SendingTheChange = "sending-the-change",
    /// `The session store could not be read.`
    SessionStoreCouldNotRead = "session-store-could-not-read",
    /// `Set a value`
    SetAValue = "set-a-value",
    /// `: set, read from…`
    SetReadFrom = "set-read-from",
    /// `Set, read from ….`
    SetReadFrom2 = "set-read-from-2",
    /// `Setting`
    Setting = "setting",
    /// `The setting … is named twice.`
    SettingNamedTwice = "setting-named-twice",
    /// `A setting's key is letters, digits, '_' and '-', its parts joined …`
    SettingSKeyLettersDigitsParts = "setting-s-key-letters-digits-parts",
    /// `Settings`
    Settings = "settings",
    /// `… settings`
    Settings2 = "settings-2",
    /// `… settings`
    Settings3 = "settings-3",
    /// `settings = …`
    Settings4 = "settings-4",
    /// `shadowed by the file`
    ShadowedByTheFile = "shadowed-by-the-file",
    /// `Signed client assertion (private_key_jwt)`
    SignedClientAssertionPrivateKeyJwt = "signed-client-assertion-private-key-jwt",
    /// `…Some of its settings take effect only at a restart: ….`
    SomeSettingsTakeEffectOnlyRestart = "some-settings-take-effect-only-restart",
    /// `State`
    State = "state",
    /// `running`
    StateRunning = "state-running",
    /// `unknown`
    StateUnknown = "state-unknown",
    /// `…It stays running, and the gateway no longer rolls it back.`
    StaysRunningGatewayNoLongerRolls = "stays-running-gateway-no-longer-rolls",
    /// `…It still takes an approval before it applies.`
    StillTakesApprovalBeforeApplies = "still-takes-approval-before-applies",
    /// `Store`
    Store = "store",
    /// `Store a new version`
    StoreANewVersion = "store-a-new-version",
    /// `Store, immutable`
    StoreImmutable = "store-immutable",
    /// `Store the version`
    StoreTheVersion = "store-the-version",
    /// `Store this version? A stored version cannot be changed or removed …`
    StoreVersionStoredVersionCannotChanged = "store-version-stored-version-cannot-changed",
    /// `Stored queries`
    StoredQueries = "stored-queries",
    /// `The stored queries come from the read-only definition files: …`
    StoredQueriesComeFromReadOnly = "stored-queries-come-from-read-only",
    /// `stored-query store`
    StoredQueryStore = "stored-query-store",
    /// `Stored-query versions by name`
    StoredQueryVersionsName = "stored-query-versions-name",
    /// `A stored version is immutable: a change is a new version, and none …`
    StoredVersionImmutableChangeNewVersion = "stored-version-immutable-change-new-version",
    /// `Such as oauth2.client_secret.`
    SuchOauth2ClientSecret = "such-oauth2-client-secret",
    /// `The system the endpoint is told the ehr_id in, such as …`
    SystemEndpointToldEhrIdSuch = "system-endpoint-told-ehr-id-such",
    /// `System id`
    SystemId = "system-id",
    /// `The system_id is not an openEHR UID.`
    SystemIdNotOpenehrUid = "system-id-not-openehr-uid",
    /// `text/plain; charset=utf-8`
    TextPlainCharsetUtf8 = "text-plain-charset-utf-8",
    /// `the configuration file`
    TheConfigurationFile = "the-configuration-file",
    /// `The deadline has passed`
    TheDeadlineHasPassed = "the-deadline-has-passed",
    /// `…The gateway says: ….`
    TheGatewaySays = "the-gateway-says",
    /// `The issuer`
    TheIssuer = "the-issuer",
    /// `The token endpoint`
    TheTokenEndpoint = "the-token-endpoint",
    /// `this new setting`
    ThisNewSetting = "this-new-setting",
    /// `TLS`
    Tls = "tls",
    /// `TLS client certificate and key (PEM)`
    TlsClientCertificateKeyPem = "tls-client-certificate-key-pem",
    /// `TLS client certificate only`
    TlsClientCertificateOnly = "tls-client-certificate-only",
    /// `TLS client certificate (tls_client_auth)`
    TlsClientCertificateTlsClientAuth = "tls-client-certificate-tls-client-auth",
    /// `Token endpoint`
    TokenEndpoint = "token-endpoint",
    /// `Token exchange`
    TokenExchange = "token-exchange",
    /// `TOML`
    Toml = "toml",
    /// `The TOML is larger than … bytes.`
    TomlLargerThanBytes = "toml-larger-than-bytes",
    /// `Trust roots file`
    TrustRootsFile = "trust-roots-file",
    /// `Type the value of …, or choose Keep.`
    TypeValueChooseKeep = "type-value-choose-keep",
    /// `… Unless an operator confirms it by…`
    UnlessOperatorConfirms = "unless-operator-confirms",
    /// `User name`
    UserName = "user-name",
    /// `Value`
    Value = "value",
    /// `The value of … does not read as the setting's type.`
    ValueDoesNotReadSettingS = "value-does-not-read-setting-s",
    /// `Value, never shown again`
    ValueNeverShownAgain = "value-never-shown-again",
    /// `Value of …`
    ValueOf = "value-of",
    /// `Version`
    Version = "version",
    /// `Versions`
    Versions = "versions",
    /// `View`
    View = "view",
    /// `What it changes against the running revision …:`
    WhatChangesAgainstRunningRevision = "what-changes-against-running-revision",
    /// `What it changes against the running revision …:`
    WhatChangesAgainstRunningRevision2 = "what-changes-against-running-revision-2",
    /// `What revision … changes, by key`
    WhatRevisionChangesKey = "what-revision-changes-key",
    /// `…Your access token may carry no operator scope.`
    YourAccessTokenMayCarryNo = "your-access-token-may-carry-no",
}
