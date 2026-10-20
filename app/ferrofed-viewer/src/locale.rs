// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The locale the console renders in, and the catalog of its own words.
//!
//! The console's words are messages of the Fluent catalogs under the crate's
//! `locales/` directory, one directory per language
//! (`locales/en/ferrofed-viewer.ftl`), embedded at build time by
//! `fluent_templates::static_loader!`, the mechanism `ferrohealth_ui`
//! catalogs its own words with. English is the one catalog that ships and the
//! one every lookup falls back to; a language joins as one catalog file and
//! no code change.
//!
//! The server chooses the locale once per request: the best language of the
//! request's `Accept-Language` the catalog holds, else the configured
//! `[server] locale`. [`crate::app::App`] provides it to both halves through
//! `ferrohealth_ui::i18n::provide_locale`, so the console's words and the
//! components' words follow the same choice, and the document writes it as
//! `<html lang>` (WCAG 2.2 SC 3.1.1). No specification governs the catalog
//! layout or the choice: our own design.

use ferrohealth_ui::i18n::{Arg, Locale, Message, locale};
use fluent_templates::Loader;

fluent_templates::static_loader! {
    static CATALOG = {
        locales: "./locales",
        fallback_language: "en",
        // NOTE: Fluent's guide, "Bidirectionality": the isolation marks around a
        // placeable are for mixed-direction text; the English catalog draws none.
        customise: |bundle| bundle.set_use_isolating(false),
    };
}

/// The console's catalogs, every language it ships.
#[must_use]
pub fn catalog() -> &'static (dyn Loader + Send + Sync) {
    &*CATALOG
}

/// Declares the console's messages: each variant names one message of the
/// catalog by its id.
macro_rules! messages {
    ($($(#[$meta:meta])+ $name:ident = $id:literal,)+) => {
        /// A string the console draws, by its message in the catalog.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Msg {
            $($(#[$meta])+ $name,)+
        }

        impl Msg {
            /// Every message, in declaration order.
            pub const ALL: &[Self] = &[$(Self::$name,)+];

            /// The message's id in the catalog.
            #[must_use]
            pub const fn id(self) -> &'static str {
                match self {
                    $(Self::$name => $id,)+
                }
            }
        }
    };
}

messages! {
    /// The title of the page for a session store the console cannot read.
    UnreadableTitle = "unreadable-title",
    /// The heading of that page.
    UnreadableHeading = "unreadable-heading",
    /// The fault that page states.
    UnreadableFault = "unreadable-fault",
    /// That page's way to ask again.
    UnreadableRetry = "unreadable-retry",
    /// The title and heading of the page for a path the console does not serve.
    NotFoundTitle = "not-found-title",
    /// That page's way back.
    NotFoundBack = "not-found-back",
    /// `The product, the gateway it reads, and who makes it.`
    ProductGatewayReadsWhoMakes = "product-gateway-reads-who-makes",
    /// `About`
    About = "about",
    /// `Website`
    Website = "website",
    /// `Contact`
    Contact = "contact",
    /// `Postal address`
    PostalAddress = "postal-address",
    /// `Name`
    Name = "name",
    /// `Manufacturer`
    Manufacturer = "manufacturer",
    /// `Gateway`
    Gateway = "gateway",
    /// `Purpose`
    Purpose = "purpose",
    /// `Version`
    Version = "version",
    /// `Product`
    Product = "product",
    /// `Sign in to see the gateway this console reads.`
    SignSeeGatewayConsoleReads = "sign-see-gateway-console-reads",
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
    /// `Sign in again`
    SignInAgain = "sign-in-again",
    /// `no code`
    NoCode = "no-code",
    ///  Your access token may carry no operator scope.
    YourAccessTokenMayCarryNo = "your-access-token-may-carry-no",
    /// `Members, comma-separated, or * for every active member`
    MembersCommaSeparatedEveryActiveMember = "members-comma-separated-every-active-member",
    /// `Stored query name`
    StoredQueryName = "stored-query-name",
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
    /// `The form is ready once the page has loaded.`
    FormReadyOncePageHasLoaded = "form-ready-once-page-has-loaded",
    /// `Cancel`
    Cancel = "cancel",
    /// `Learn more`
    LearnMore = "learn-more",
    /// `Operator`
    Operator = "operator",
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
    /// `Configuration`
    Configuration = "configuration",
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
    /// `Value`
    Value = "value",
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
    Endpoint = "endpoint",
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
    /// `State`
    State = "state",
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
    /// `Members`
    Members = "members",
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
    /// `Node`
    Node = "node",
    /// `Organisation name`
    OrganisationName = "organisation-name",
    /// `Organisation id`
    OrganisationId = "organisation-id",
    /// `Organisation`
    Organisation = "organisation",
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
    /// `Endpoints`
    Endpoints = "endpoints",
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
    /// `System id`
    SystemId = "system-id",
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
    /// `Console`
    Console = "console",
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
    /// `Stored queries`
    StoredQueries = "stored-queries",
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
    /// `AQL`
    Aql = "aql",
    /// `major.minor.patch, such as 1.0.0.`
    MajorMinorPatchSuch10 = "major-minor-patch-such-1-0",
    /// `A namespace and a name, such as org.example::vital-signs.`
    NamespaceNameSuchOrgExampleVital = "namespace-name-such-org-example-vital",
    /// `Qualified name`
    QualifiedName = "qualified-name",
    /// `Stored-query versions by name`
    StoredQueryVersionsName = "stored-query-versions-name",
    /// `Versions`
    Versions = "versions",
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
    /// `All incidents`
    AllIncidents = "all-incidents",
    /// `Recent incidents`
    RecentIncidents = "recent-incidents",
    /// `At`
    At = "at",
    /// `None since the gateway started.`
    NoneSinceGatewayStarted = "none-since-gateway-started",
    /// `No incidents`
    NoIncidents = "no-incidents",
    /// `All members`
    AllMembers = "all-members",
    /// `Member health`
    MemberHealth = "member-health",
    /// `The gateway declares no member endpoint.`
    GatewayDeclaresNoMemberEndpoint = "gateway-declares-no-member-endpoint",
    /// `No members`
    NoMembers = "no-members",
    /// `Median latency`
    MedianLatency = "median-latency",
    /// `Health`
    Health = "health",
    /// `Kind`
    Kind = "kind",
    /// `Running revision`
    RunningRevision = "running-revision",
    /// `Open incidents`
    OpenIncidents = "open-incidents",
    /// `Nodes up`
    NodesUp = "nodes-up",
    /// `Members active`
    MembersActive = "members-active",
    /// `not shown`
    NotShown = "not-shown",
    /// `not yet observed`
    NotYetObserved = "not-yet-observed",
    /// `all up`
    AllUp = "all-up",
    /// `no nodes`
    NoNodes = "no-nodes",
    /// `not reported`
    NotReported = "not-reported",
    /// `Loading the dashboard.`
    LoadingTheDashboard = "loading-the-dashboard",
    /// `Dashboard`
    Dashboard = "dashboard",
    /// `The state of the federation, as this console reads it.`
    StateFederationConsoleReads = "state-federation-console-reads",
    /// `Sign in to the console`
    SignConsole = "sign-console",
    /// `Sign in`
    SignIn = "sign-in",
    /// `No rows.`
    NoRows = "no-rows",
    /// `1 row`
    N1Row = "n-1-row",
    /// `Every endpoint the gateway reports`
    EveryEndpointGatewayReports = "every-endpoint-gateway-reports",
    /// `Error`
    Error = "error",
    /// `Rows`
    Rows = "rows",
    /// `Latency`
    Latency = "latency",
    /// `Status`
    Status = "status",
    /// `none`
    None = "none",
    /// `Endpoint details`
    EndpointDetails = "endpoint-details",
    /// `Nodes`
    Nodes = "nodes",
    /// `Every node in scope answered.`
    EveryNodeScopeAnswered = "every-node-scope-answered",
    ///  The nodes below say what each one did.
    NodesBelowSayWhatEachOne = "nodes-below-say-what-each-one",
    ///  The gateway returned no rows.
    GatewayReturnedNoRows = "gateway-returned-no-rows",
    ///  A node in scope answered with an error, so the gateway returned no rows.
    NodeScopeAnsweredErrorSoGateway = "node-scope-answered-error-so-gateway",
    ///  A node in scope was asked and did not answer, so the gateway returned no rows.
    NodeScopeWasAskedDidNot = "node-scope-was-asked-did-not",
    /// `Leave all clear to let the gateway choose.`
    LeaveAllClearLetGatewayChoose = "leave-all-clear-let-gateway-choose",
    /// `Best effort`
    BestEffort = "best-effort",
    /// `The gateway's default`
    TheGatewaySDefault = "the-gateway-s-default",
    /// `Deduplication`
    Deduplication = "deduplication",
    /// `Federation options`
    FederationOptions = "federation-options",
    /// `This gateway does not offer best effort.`
    GatewayDoesNotOfferBestEffort = "gateway-does-not-offer-best-effort",
    /// `Answer with the rows that came back when a node does not answer.`
    AnswerRowsCameBackWhenNode = "answer-rows-came-back-when-node",
    /// `The gateway declares no organisation.`
    GatewayDeclaresNoOrganisation = "gateway-declares-no-organisation",
    /// `Organisations`
    Organisations = "organisations",
    /// `The rows to return. Empty leaves it to the gateway.`
    RowsReturnEmptyLeavesGateway = "rows-return-empty-leaves-gateway",
    /// `Fetch`
    Fetch = "fetch",
    /// `The rows to skip. Empty starts at the first row.`
    RowsSkipEmptyStartsFirstRow = "rows-skip-empty-starts-first-row",
    /// `Offset`
    Offset = "offset",
    /// `Letters, digits and underscores`
    LettersDigitsAndUnderscores = "letters-digits-and-underscores",
    /// `Name a patient here. Each value is sent as a string, in the request body.`
    NamePatientHereEachValueSent = "name-patient-here-each-value-sent",
    /// `Add parameter`
    AddParameter = "add-parameter",
    /// `Parameters`
    Parameters = "parameters",
    /// `Version (optional)`
    VersionOptional = "version-optional",
    /// `Name a patient as a parameter, never in the AQL text.`
    NamePatientParameterNeverAqlText = "name-patient-parameter-never-aql-text",
    /// `Run query`
    RunQuery = "run-query",
    /// `Stored query`
    StoredQuery = "stored-query",
    /// `Query kind`
    QueryKind = "query-kind",
    /// `Loading what the gateway offers a query.`
    LoadingWhatGatewayOffersQuery = "loading-what-gateway-offers-query",
    /// `Query`
    Query = "query",
    /// `Run AQL or a stored query across the federation.`
    RunAqlStoredQueryAcrossFederation = "run-aql-stored-query-across-federation",
    /// `Query console`
    QueryConsole = "query-console",
    /// `Next`
    Next = "next",
    /// `Previous`
    Previous = "previous",
    /// `Pages of rows`
    PagesOfRows = "pages-of-rows",
    /// `The federated result set as JSON`
    FederatedResultSetJson = "federated-result-set-json",
    /// `Write AQL or name a stored query, then run it.`
    WriteAqlNameStoredQueryThen = "write-aql-name-stored-query-then",
    /// `No query has run yet.`
    NoQueryHasRunYet = "no-query-has-run-yet",
    /// `Running the query.`
    RunningTheQuery = "running-the-query",
    /// `Results`
    Results = "results",
    /// `Table`
    Table = "table",
    /// `Raw`
    Raw = "raw",
    /// `Answer`
    Answer = "answer",
    /// `an error object the console cannot print`
    ErrorObjectConsoleCannotPrint = "error-object-console-cannot-print",
    /// `The console cannot print this result set as JSON.`
    ConsoleCannotPrintResultSetJson = "console-cannot-print-result-set-json",
    /// `dedup mode`
    DedupMode = "dedup-mode",
    /// `Choose an AQL query or a stored query.`
    ChooseAqlQueryStoredQuery = "choose-aql-query-stored-query",
    /// `Name the stored query to run.`
    NameStoredQueryRun = "name-stored-query-run",
    /// `Write the AQL query to run.`
    WriteAqlQueryRun = "write-aql-query-run",
    /// `Views`
    Views = "views",
    /// `Theme not kept`
    ThemeNotKept = "theme-not-kept",
    /// `the console refused it`
    TheConsoleRefusedIt = "the-console-refused-it",
    /// `Development deployment`
    DevelopmentDeployment = "development-deployment",
    ///  Patients resolve through the static development cross-reference.
    PatientsResolveThroughStaticDevelopmentCross = "patients-resolve-through-static-development-cross",
    /// `this gateway runs the development profile and must not hold real patient data.`
    GatewayRunsDevelopmentProfileMustNot = "gateway-runs-development-profile-must-not",
    /// `Gateway UNKNOWN`
    GatewayUnknown = "gateway-unknown",
    /// `Gateway DOWN`
    GatewayDown = "gateway-down",
    /// `Gateway refused 401`
    GatewayRefused401 = "gateway-refused-401",
    /// `Gateway UP`
    GatewayUp = "gateway-up",
    /// `Signed in to this console`
    SignedConsole = "signed-console",
    /// `Queries`
    Queries = "queries",
    /// `Federation`
    Federation = "federation",
    /// `Overview`
    Overview = "overview",
    /// `Self-description`
    SelfDescription = "self-description",
    /// `Integrity`
    Integrity = "integrity",
    /// `Routing`
    Routing = "routing",
    /// `Declared endpoints`
    DeclaredEndpoints = "declared-endpoints",
    /// `Membership`
    Membership = "membership",
    /// `Fixes until`
    FixesUntil = "fixes-until",
    /// `no end date`
    NoEndDate = "no-end-date",
    /// `declared in a form the console cannot read`
    DeclaredFormConsoleCannotRead = "declared-form-console-cannot-read",
    /// `not declared`
    NotDeclared = "not-declared",
    /// `Decomposable aggregates`
    DecomposableAggregates = "decomposable-aggregates",
    /// `Offset strategy`
    OffsetStrategy = "offset-strategy",
    /// `Timeout policy`
    TimeoutPolicy = "timeout-policy",
    /// `Overall`
    Overall = "overall",
    /// `Per node`
    PerNode = "per-node",
    /// `On a localization failure`
    OnALocalizationFailure = "on-a-localization-failure",
    /// `Template upload fan-out`
    TemplateUploadFanOut = "template-upload-fan-out",
    /// `Stored-query fan-out`
    StoredQueryFanOut = "stored-query-fan-out",
    /// `Stored-query registry`
    StoredQueryRegistry = "stored-query-registry",
    /// `none declared`
    NoneDeclared = "none-declared",
    /// `Deduplication header`
    DeduplicationHeader = "deduplication-header",
    /// `Deduplication modes`
    DeduplicationModes = "deduplication-modes",
    /// `AQL fan-out`
    AqlFanOut = "aql-fan-out",
    /// `Demographic`
    Demographic = "demographic",
    /// `Definition`
    Definition = "definition",
    /// `EHR`
    Ehr = "ehr",
    /// `JWK Set`
    JwkSet = "jwk-set",
    /// `Member endpoints`
    MemberEndpoints = "member-endpoints",
    /// `Specification version`
    SpecificationVersion = "specification-version",
    /// `not offered`
    NotOffered = "not-offered",
    /// `The OPTIONS document`
    TheOptionsDocument = "the-options-document",
    /// `ITS-REST areas`
    ItsRestAreas = "its-rest-areas",
    /// `Budgets and paging`
    BudgetsAndPaging = "budgets-and-paging",
    /// `Capabilities`
    Capabilities = "capabilities",
    /// `Loading the self-description.`
    LoadingTheSelfDescription = "loading-the-self-description",
    /// `What the gateway declares of itself in OPTIONS.`
    WhatGatewayDeclaresItselfOptions = "what-gateway-declares-itself-options",
    /// `Collisions cleared recently, newest first`
    CollisionsClearedRecentlyNewestFirst = "collisions-cleared-recently-newest-first",
    /// `Members that claimed it`
    MembersThatClaimedIt = "members-that-claimed-it",
    /// `not a bare UUID`
    NotABareUuid = "not-a-bare-uuid",
    /// `Cleared at`
    ClearedAt = "cleared-at",
    /// `No collision cleared since the gateway started.`
    NoCollisionClearedSinceGatewayStarted = "no-collision-cleared-since-gateway-started",
    /// `Collisions cleared recently`
    CollisionsClearedRecently = "collisions-cleared-recently",
    /// `Recent incidents, newest first`
    RecentIncidentsNewestFirst = "recent-incidents-newest-first",
    /// `Description`
    Description = "description",
    /// `None of the incidents this replica keeps is open.`
    NoneIncidentsReplicaKeepsOpen = "none-incidents-replica-keeps-open",
    /// `No open incidents`
    NoOpenIncidents = "no-open-incidents",
    /// `No collision held here was cleared since it was found.`
    NoCollisionHeldHereWasCleared = "no-collision-held-here-was-cleared",
    /// `No resolved incidents`
    NoResolvedIncidents = "no-resolved-incidents",
    /// `Resolved`
    Resolved = "resolved",
    /// `Open`
    Open = "open",
    /// `Incidents by state`
    IncidentsByState = "incidents-by-state",
    /// `Incidents since the gateway started`
    IncidentsSinceGatewayStarted = "incidents-since-gateway-started",
    /// `Count`
    Count = "count",
    /// `Loading the incidents.`
    LoadingTheIncidents = "loading-the-incidents",
    /// `The incidents this replica keeps, and the collisions cleared.`
    IncidentsReplicaKeepsCollisionsCleared = "incidents-replica-keeps-collisions-cleared",
    /// `The gateway did not name the replica that answered.`
    GatewayDidNotNameReplicaAnswered = "gateway-did-not-name-replica-answered",
    /// `routing table`
    RoutingTable = "routing-table",
    /// `No such member`
    NoSuchMember = "no-such-member",
    /// `Endpoints of this member`
    EndpointsOfThisMember = "endpoints-of-this-member",
    /// `Loading the member.`
    LoadingTheMember = "loading-the-member",
    /// `Other dependencies`
    OtherDependencies = "other-dependencies",
    /// `Dependency`
    Dependency = "dependency",
    /// `Loading the members.`
    LoadingTheMembers = "loading-the-members",
    /// `Each member endpoint and its health, as one replica observes it.`
    EachMemberEndpointHealthOneReplica = "each-member-endpoint-health-one-replica",
    /// `Sign in to see this view.`
    SignSeeView = "sign-see-view",
    /// `Pages`
    Pages = "pages",
    /// `Filter`
    Filter = "filter",
    /// `The creating_system_id routing table`
    CreatingSystemIdRoutingTable = "creating-system-id-routing-table",
    /// `No row on this page matches the filter.`
    NoRowPageMatchesFilter = "no-row-page-matches-filter",
    /// `Source`
    Source = "source",
    /// `Loading the routing table.`
    LoadingTheRoutingTable = "loading-the-routing-table",
    /// `Where each creating_system_id routes, and on what ground.`
    WhereEachCreatingSystemIdRoutes = "where-each-creating-system-id-routes",
    /// `Loading the stored query.`
    LoadingTheStoredQuery = "loading-the-stored-query",
    /// `Loading the stored queries.`
    LoadingTheStoredQueries = "loading-the-stored-queries",
    /// `The queries the gateway holds, by namespace.`
    QueriesGatewayHoldsNamespace = "queries-gateway-holds-namespace",
    /// `No such stored query`
    NoSuchStoredQuery = "no-such-stored-query",
    /// `Back to the stored queries`
    BackStoredQueries = "back-stored-queries",
    /// `stored queries`
    StoredQueries2 = "stored-queries-2",
    /// `shown`
    Shown = "shown",
    /// `Details`
    Details = "details",
    /// `Stored`
    Stored = "stored",
    /// `Namespace`
    Namespace = "namespace",
    /// `Open in query console`
    OpenInQueryConsole = "open-in-query-console",
    /// `No namespace`
    NoNamespace = "no-namespace",
    /// `1 version`
    N1Version = "n-1-version",
    /// `Namespaces`
    Namespaces = "namespaces",
    /// `No stored queries`
    NoStoredQueries = "no-stored-queries",
    /// `The gateway holds no stored query.`
    GatewayHoldsNoStoredQuery = "gateway-holds-no-stored-query",
    /// The About page's purpose line.
    AboutPurpose = "about-purpose",
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
    /// The gateway refused this part: `$status` (`$code`).
    GatewayRefusedPart = "gateway-refused-part",
    /// The gateway did not accept your sign-in (`$code`).
    GatewayDidNotAcceptYourSign = "gateway-did-not-accept-your-sign",
    /// Not shown: `$fault`.
    NotShown2 = "not-shown-2",
    /// `$down down`
    Down = "down",
    /// `$active of $total`
    Of = "of",
    /// `$up of $total`
    Of2 = "of-2",
    /// revision `$pending` pending
    RevisionPending = "revision-pending",
    /// `$latency ms`
    Ms = "ms",
    /// failed `$status`
    Failed = "failed",
    /// `$replied of 1 node answered`
    Of1NodeAnswered = "of-1-node-answered",
    /// `$replied of $in_scope nodes answered`
    OfNodesAnswered = "of-nodes-answered",
    /// `$elapsed ms`
    Ms2 = "ms-2",
    /// , `$rows` rows suppressed
    RowsSuppressed = "rows-suppressed",
    /// Deduplication: `$mode``$suppressed`.
    Deduplication2 = "deduplication-2",
    /// The gateway failed the query: `$status`.
    GatewayFailedQuery = "gateway-failed-query",
    /// `$latency ms`
    Ms3 = "ms-3",
    /// `$latency ms`
    Ms4 = "ms-4",
    /// `$count rows`
    Rows2 = "rows-2",
    /// `$count rows`
    Rows3 = "rows-3",
    /// Rows `$first` to `$end` of `$rows`
    RowsToOf = "rows-to-of",
    /// #`$value0` table
    Table2 = "table-2",
    /// The `$field` cannot be sent as a header.
    CannotSentHeader = "cannot-sent-header",
    /// The `$field` is a whole number from 0.
    WholeNumberFrom0 = "whole-number-from-0",
    /// Parameter `$number` is not name=value.
    ParameterNotNameValue = "parameter-not-name-value",
    /// Parameter `$number` has no name of letters, digits and underscores.
    ParameterHasNoNameLettersDigits = "parameter-has-no-name-letters-digits",
    /// Parameter `$number` cannot be sent.
    ParameterCannotBeSent = "parameter-cannot-be-sent",
    /// Parameter `$number` names a parameter given before.
    ParameterNamesParameterGivenBefore = "parameter-names-parameter-given-before",
    /// · v`$version` · `$replica`
    V = "v",
    /// · v`$version`
    V2 = "v-2",
    /// · spec `$spec_version` · `$federation`
    Spec = "spec",
    /// Gateway refused `$status`
    GatewayRefused = "gateway-refused",
    /// offered, by `$value0`
    OfferedBy = "offered-by",
    /// `$per_node_ms ms`
    Ms5 = "ms-5",
    /// `$overall_ms ms`
    Ms6 = "ms-6",
    /// Answered by replica `$replica`.
    AnsweredByReplica = "answered-by-replica",
    /// the `$what` by a replica that has no name
    ReplicaHasNoName = "replica-has-no-name",
    /// the `$what` by replica `$replica`
    TheByReplica = "the-by-replica",
    /// the `$what` by a replica that gave no name
    ReplicaGaveNoName = "replica-gave-no-name",
    /// Each part names its replica: `$parts`.
    EachPartNamesReplica = "each-part-names-replica",
    /// `$latency ms`
    Ms7 = "ms-7",
    /// Member `$value0`
    Member = "member",
    /// A member of `$organisation`, with its endpoints and their health.
    MemberEndpointsTheirHealth = "member-endpoints-their-health",
    /// No row on this page, of `$total` in all.
    NoRowPageAll = "no-row-page-all",
    /// Rows `$offset` to `$end` of `$total`.
    RowsToOf2 = "rows-to-of-2",
    /// The gateway did not accept your sign-in (`$code`).
    GatewayDidNotAcceptYourSign2 = "gateway-did-not-accept-your-sign-2",
    /// The gateway refused this view: `$status` (`$code`).`$hint`
    GatewayRefusedView = "gateway-refused-view",
    /// `$kept of $on_page rows on this page.`
    RowsPage = "rows-page",
    /// `$n versions`
    Versions2 = "versions-2",
    /// The gateway holds no version `$asked` of this query. The newest is shown.
    GatewayHoldsNoVersionQueryNewest = "gateway-holds-no-version-query-newest",
    /// Version `$version`, stored `$saved`.
    VersionStored = "version-stored",
    /// The AQL of version `$version`
    TheAqlOfVersion = "the-aql-of-version",
    /// The gateway holds no stored query named `$name`.
    GatewayHoldsNoStoredQueryNamed = "gateway-holds-no-stored-query-named",
    /// A capability the gateway offers.
    Offered = "offered",
    /// `Client assertion audience`
    ClientAssertionAudience = "client-assertion-audience",
    /// `Signed client assertion (private_key_jwt)`
    SignedClientAssertionPrivateKeyJwt = "signed-client-assertion-private-key-jwt",
    /// A resource that runs.
    StateRunning = "state-running",
    /// A state the console does not know.
    StateUnknown = "state-unknown",
    /// An incident that is resolved.
    StateResolved = "state-resolved",
    /// An incident that is open.
    StateOpen = "state-open",
    /// A node that refused.
    StateRefused = "state-refused",
    /// The label of the member's node identifier.
    NodeIdLabel = "node-id-label",
    /// The hint that names the configuration key `$key` a field writes.
    ConfigurationKey = "configuration-key",
    /// The sentence of that page.
    NotFoundBody = "not-found-body",
    /// The object a stored-query action reports on.
    StoredQueryStore = "stored-query-store",
    /// `404: the gateway declares no member at this address.`
    NoMemberAtAddress = "no-member-at-address",
    /// The badge on a stored query the gateway serves read-only.
    ReadOnlyBadge = "read-only-badge",
}

/// The console's message `msg` in the page's locale.
#[must_use]
pub fn tr(msg: Msg) -> String {
    tr_in(catalog(), &locale(), msg)
}

/// The console's message `msg` in the page's locale, with the placeables
/// `args` filled in.
#[must_use]
pub fn tr_with(msg: Msg, args: &[(&'static str, Arg)]) -> String {
    locale().lookup_with(catalog(), msg.id(), args)
}

/// The console's message `msg` in `locale`, with the placeables `args`
/// filled in: for a handler or a task, where no page tree is in reach.
#[must_use]
pub fn tr_in_with(locale: &Locale, msg: Msg, args: &[(&'static str, Arg)]) -> String {
    locale.lookup_with(catalog(), msg.id(), args)
}

/// The console's message `msg` as a [`Message`] a component looks up as it
/// draws, such as a listing's noun or an editor's title.
#[must_use]
pub fn message(msg: Msg) -> Message {
    Message::new(catalog(), msg.id())
}

/// The message `msg` of `catalog` in `locale`.
#[must_use]
pub fn tr_in(catalog: &dyn Loader, locale: &Locale, msg: Msg) -> String {
    locale.lookup(catalog, msg.id())
}

/// The console's message `msg` in the locale of the request being served:
/// for a server function, which runs outside the page tree.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn tr_served(msg: Msg) -> String {
    requested().lookup(catalog(), msg.id())
}

/// The console's message `msg` with the placeables `args` filled in, in the
/// locale of the request being served.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn tr_served_with(msg: Msg, args: &[(&'static str, Arg)]) -> String {
    requested().lookup_with(catalog(), msg.id(), args)
}

/// The console's message `msg`: in the browser, where a server function's
/// body is never compiled, the page's locale.
#[cfg(target_arch = "wasm32")]
#[must_use]
pub fn tr_served(msg: Msg) -> String {
    tr(msg)
}

/// The console's message `msg` with the placeables `args` filled in, in the
/// page's locale: the browser's half of [`tr_served_with`].
#[cfg(target_arch = "wasm32")]
#[must_use]
pub fn tr_served_with(msg: Msg, args: &[(&'static str, Arg)]) -> String {
    tr_with(msg, args)
}

/// The locale of the request being served: the best language of its
/// `Accept-Language` the catalog holds, else the configured one, English
/// with no configuration in reach.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn requested() -> Locale {
    use leptos::context::use_context;
    let configured = use_context::<crate::server::ViewerState>()
        .map_or_else(Locale::english, |state| state.settings().locale.clone());
    let header = use_context::<http::request::Parts>()
        .and_then(|parts| {
            parts
                .headers
                .get(http::header::ACCEPT_LANGUAGE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        })
        .unwrap_or_default();
    Locale::negotiate_with(&header, catalog(), configured)
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::collections::HashMap;
    use std::path::Path;

    use ferrohealth_ui::i18n::Locale;
    use fluent_templates::fluent_bundle::FluentValue;

    use super::{Msg, catalog, tr_in};

    fluent_templates::static_loader! {
        static THROWAWAY = {
            locales: "./tests/locales",
            fallback_language: "qaa",
            customise: |bundle| bundle.set_use_isolating(false),
        };
    }

    /// Every placeable the catalog at `path` names, each set to a number, so
    /// a message that selects a form by one formats whole.
    fn placeables(path: &Path) -> HashMap<Cow<'static, str>, FluentValue<'static>> {
        let source = std::fs::read_to_string(path).expect("the catalog");
        source
            .split("{ $")
            .skip(1)
            .filter_map(|rest| rest.split([' ', '}']).next())
            .map(|name| (Cow::Owned(name.to_owned()), FluentValue::from(3)))
            .collect()
    }

    /// The directories under `locales/`, each a language.
    fn catalog_directories(root: &Path) -> Vec<String> {
        let mut found: Vec<String> = std::fs::read_dir(root)
            .expect("the locales directory")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        found.sort();
        found
    }

    #[test]
    fn every_catalog_directory_is_a_language_holding_every_message() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("locales");
        let found = catalog_directories(&root);
        let mut shipped: Vec<String> = catalog().locales().map(ToString::to_string).collect();
        shipped.sort();
        assert_eq!(found, shipped, "a catalog directory is a language");
        assert!(
            found.contains(&String::from("en")),
            "English is the reference"
        );
        for tag in found {
            let language = tag.parse().expect("a language");
            let args = placeables(&root.join(&tag).join("ferrofed-viewer.ftl"));
            for msg in Msg::ALL {
                let words = catalog().try_lookup_with_args(&language, msg.id(), &args);
                assert!(
                    words.is_some_and(|words| !words.trim().is_empty()),
                    "{tag} holds {}",
                    msg.id()
                );
            }
        }
    }

    #[test]
    fn a_throwaway_catalog_changes_the_words_with_no_code_change() {
        let english = Locale::english();
        assert_eq!("Not found", tr_in(catalog(), &english, Msg::NotFoundTitle));
        let throwaway = Locale::parse("qaa").expect("a private-use language");
        assert_eq!(
            "Qot qound",
            tr_in(&*THROWAWAY, &throwaway, Msg::NotFoundTitle)
        );
        assert_eq!(
            "Back to the console",
            tr_in(catalog(), &throwaway, Msg::NotFoundBack),
            "the shipped catalogs fall back to English"
        );
    }
}
