// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The messages of `locales/en/settings.ftl`.

messages! {
    /// `Category map by archetype`
    SettingArchetypeMap = "setting-archetype-map",
    /// `Archetype ids, each mapped to the categories of the data under it.`
    SettingArchetypeMapDesc = "setting-archetype-map-desc",
    /// `Audience`
    SettingAudience = "setting-audience",
    /// `The audiences a caller's token must name for this gateway.`
    SettingAudienceDesc = "setting-audience-desc",
    /// `Backend`
    SettingBackend = "setting-backend",
    /// `Where the stored queries are kept: redb, postgres or files.`
    SettingBackendDesc = "setting-backend-desc",
    /// `Best effort`
    SettingBestEffort = "setting-best-effort",
    /// `A client may ask for the rows that came back when a node does not answer.`
    SettingBestEffortDesc = "setting-best-effort-desc",
    /// `Connection`
    SettingConnection = "setting-connection",
    /// `The PostgreSQL connection string, a secret, named by the file it is read from.`
    SettingConnectionDesc = "setting-connection-desc",
    /// `Disclose exclusions`
    SettingDisclose = "setting-disclose",
    /// `Name a node the consent pre-filter excludes as consent-denied. Off, it reads as a node without the patient, as Regulation (EU) 2025/327 Art 8 may require.`
    SettingDiscloseDesc = "setting-disclose-desc",
    /// `In an emergency`
    SettingEmergency = "setting-emergency",
    /// `Whether the consent pre-filter still applies when the caller declares an emergency purpose: apply, or pass-to-node.`
    SettingEmergencyDesc = "setting-emergency-desc",
    /// `Emergency purposes`
    SettingEmergencyPurposes = "setting-emergency-purposes",
    /// `Purposes of use that mark an emergency access.`
    SettingEmergencyPurposesDesc = "setting-emergency-purposes-desc",
    /// `Base`
    SettingFhirBase = "setting-fhir-base",
    /// `Where the exchange-format face is served, beside the openEHR API.`
    SettingFhirBaseDesc = "setting-fhir-base-desc",
    /// `Introspection secret`
    SettingIntrospectionSecret = "setting-introspection-secret",
    /// `The secret the gateway authenticates to this issuer's introspection endpoint with, named by the file it is read from.`
    SettingIntrospectionSecretDesc = "setting-introspection-secret-desc",
    /// `Key set file`
    SettingKeySetFile = "setting-key-set-file",
    /// `The file this issuer's signing keys are read from, when they are not fetched.`
    SettingKeySetFileDesc = "setting-key-set-file-desc",
    /// `Localizer budget`
    SettingLocalizerBudget = "setting-localizer-budget",
    /// `How long the localizer may take, in milliseconds, out of the overall timeout.`
    SettingLocalizerBudgetDesc = "setting-localizer-budget-desc",
    /// `When the localizer fails`
    SettingLocalizerFails = "setting-localizer-fails",
    /// `closed asks no node and says why; ask-all asks every node.`
    SettingLocalizerFailsDesc = "setting-localizer-fails-desc",
    /// `Correlation header`
    SettingNcpCorrelation = "setting-ncp-correlation",
    /// `The request header the contact point's connector sends its correlation identifier in, recorded with the access.`
    SettingNcpCorrelationDesc = "setting-ncp-correlation-desc",
    /// `Added countries`
    SettingNcpCountries = "setting-ncp-countries",
    /// `The countries beside the 27 Member States a relayed country code may name.`
    SettingNcpCountriesDesc = "setting-ncp-countries-desc",
    /// `Country code claim`
    SettingNcpCountryCode = "setting-ncp-country-code",
    /// `The claim carrying the country that issued the professional's identification data (Implementing Regulation (EU) 2026/2099 Annex Table 1).`
    SettingNcpCountryCodeDesc = "setting-ncp-country-code-desc",
    /// `Family name claim`
    SettingNcpFamilyName = "setting-ncp-family-name",
    /// `The claim carrying the professional's family name (Annex Table 1). Set, it declares the issuer a national contact point.`
    SettingNcpFamilyNameDesc = "setting-ncp-family-name-desc",
    /// `Given name claim`
    SettingNcpGivenName = "setting-ncp-given-name",
    /// `The claim carrying the professional's given name (Annex Table 1).`
    SettingNcpGivenNameDesc = "setting-ncp-given-name-desc",
    /// `Professional's issuing authority claim`
    SettingNcpProfessionalAuthority = "setting-ncp-professional-authority",
    /// `The claim carrying the authority that issued the professional's identifier (Annex Table 1).`
    SettingNcpProfessionalAuthorityDesc = "setting-ncp-professional-authority-desc",
    /// `Provider address claim`
    SettingNcpProviderAddress = "setting-ncp-provider-address",
    /// `The claim carrying the healthcare provider's address (Annex Table 2).`
    SettingNcpProviderAddressDesc = "setting-ncp-provider-address-desc",
    /// `Provider's issuing authority claim`
    SettingNcpProviderAuthority = "setting-ncp-provider-authority",
    /// `The claim carrying the authority that issued the provider's identifier (Annex Table 2).`
    SettingNcpProviderAuthorityDesc = "setting-ncp-provider-authority-desc",
    /// `Node selection`
    SettingNodeSelection = "setting-node-selection",
    /// `Which nodes a patient's query asks: localized, the nodes the localizer names, or ask-all, every node.`
    SettingNodeSelectionDesc = "setting-node-selection-desc",
    /// `Overall timeout`
    SettingOverallTimeout = "setting-overall-timeout",
    /// `How long a whole query may take, in milliseconds. With the 1,000 ms margin it stays below each replica's own request timeout.`
    SettingOverallTimeoutDesc = "setting-overall-timeout-desc",
    /// `Paging`
    SettingPaging = "setting-paging",
    /// `bounded asks each node for offset plus limit rows and merges them within the window; reject refuses an offset.`
    SettingPagingDesc = "setting-paging-desc",
    /// `Paging window`
    SettingPagingWindow = "setting-paging-window",
    /// `The most rows, offset plus limit, one page may ask of a node; a positive number.`
    SettingPagingWindowDesc = "setting-paging-window-desc",
    /// `Per-node timeout`
    SettingPerNodeTimeout = "setting-per-node-timeout",
    /// `How long one node may take, in milliseconds: no more than the overall timeout.`
    SettingPerNodeTimeoutDesc = "setting-per-node-timeout-desc",
    /// `Operator scope`
    SettingProductScope = "setting-product-scope",
    /// `The scope a token needs for the operator surface and the admin actions.`
    SettingProductScopeDesc = "setting-product-scope-desc",
    /// `Received categories`
    SettingReceived = "setting-received",
    /// `The categories of document the face accepts.`
    SettingReceivedDesc = "setting-received-desc",
    /// `Retention`
    SettingRetention = "setting-retention",
    /// `Years each access record is kept: at least 3.`
    SettingRetentionDesc = "setting-retention-desc",
    /// `Rows per node`
    SettingRowsPerNode = "setting-rows-per-node",
    /// `The most rows read of one node for a query without LIMIT; a positive number.`
    SettingRowsPerNodeDesc = "setting-rows-per-node-desc",
    /// `Path`
    SettingStorePath = "setting-store-path",
    /// `The redb store file, or the directory the files backend reads.`
    SettingStorePathDesc = "setting-store-path-desc",
    /// `Stored-query fan-out`
    SettingStoredFanOut = "setting-stored-fan-out",
    /// `Send each new stored version on to the nodes as well.`
    SettingStoredFanOutDesc = "setting-stored-fan-out-desc",
    /// `Template upload fan-out`
    SettingTemplateFanOut = "setting-template-fan-out",
    /// `Send an uploaded template to several nodes in one request.`
    SettingTemplateFanOutDesc = "setting-template-fan-out-desc",
    /// `Category map by template`
    SettingTemplateMap = "setting-template-map",
    /// `Template ids, each mapped to the categories of the data under it.`
    SettingTemplateMapDesc = "setting-template-map-desc",
    /// `Token issuer`
    SettingTokenIssuer = "setting-token-issuer",
    /// `An issuer whose access tokens the gateway verifies.`
    SettingTokenIssuerDesc = "setting-token-issuer-desc",
    /// `Trusted issuer`
    SettingTrustedIssuer = "setting-trusted-issuer",
    /// `An issuer whose callers the gateway admits, with the policy below it.`
    SettingTrustedIssuerDesc = "setting-trusted-issuer-desc",
    /// `A setting this console does not describe.`
    SettingUnknown = "setting-unknown",
    /// `Document URL lifetime`
    SettingUrlLifetime = "setting-url-lifetime",
    /// `How long a listed document's URL stays valid, in seconds: at least 1.`
    SettingUrlLifetimeDesc = "setting-url-lifetime-desc",
    /// `Change a setting before you save.`
    ChangeASettingBeforeYouSave = "settings-change-one",
    /// `Only the settings you change are saved; a list or a table, and a secret, are changed by an import.`
    SettingsChangedOnlyHint = "settings-changed-only-hint",
    /// `Edit`
    SettingsEdit = "settings-edit",
    /// `Edit { $tab }`
    SettingsEditTitle = "settings-edit-title",
    /// `Revisions are off on this gateway: every setting is read from its configuration file and environment, and shown here read-only.`
    SettingsFileLine = "settings-file-line",
    /// `What the configuration file and the environment set, as the gateway runs it.`
    SettingsFileSubtitle = "settings-file-subtitle",
    /// `How each access record is classified and kept.`
    SettingsIntroAccessLog = "settings-intro-access-log",
    /// `Who may call: the issuers the gateway trusts, a national contact point among them. Only the configuration file sets these, so this tab is read-only.`
    SettingsIntroAuthentication = "settings-intro-authentication",
    /// `What callers learn about consent.`
    SettingsIntroConsent = "settings-intro-consent",
    /// `A draft of this tab, based on revision { $base }. Saved, it is a revision you apply as any other.`
    SettingsIntroEdit = "settings-intro-edit",
    /// `A draft of this tab. Saved, it is a revision you apply as any other.`
    SettingsIntroEditFirst = "settings-intro-edit-first",
    /// `How a query is paged, and what is sent on to the nodes.`
    SettingsIntroFanOut = "settings-intro-fan-out",
    /// `How a query fans out to the nodes and when it gives up.`
    SettingsIntroFederation = "settings-intro-federation",
    /// `The face that serves and receives the European exchange format.`
    SettingsIntroFhir = "settings-intro-fhir",
    /// `The store the stored queries are kept in; the stored versions are listed below.`
    SettingsIntroStoredQueries = "settings-intro-stored-queries",
    /// `{ $count } ids mapped`
    SettingsMapped = "settings-mapped",
    /// `{ $value } ms`
    SettingsMs = "settings-ms",
    /// `{ $value } at the next start`
    SettingsNextValue = "settings-next-value",
    /// `Not set`
    SettingsNotSet = "settings-not-set",
    /// `Off`
    SettingsOff = "settings-off",
    /// `On`
    SettingsOn = "settings-on",
    /// `Changed by an import`
    SettingsReadOnlyHere = "settings-read-only-here",
    /// `{ $value } rows`
    SettingsRows = "settings-rows",
    /// `{ $value } s`
    SettingsSeconds = "settings-seconds",
    /// `Set, read from`
    SettingsSecretSetFile = "settings-secret-set-file",
    /// `Set in the file`
    SettingsSecretSetInline = "settings-secret-set-inline",
    /// `Access log`
    SettingsTabAccessLog = "settings-tab-access-log",
    /// `Authentication`
    SettingsTabAuthentication = "settings-tab-authentication",
    /// `Consent`
    SettingsTabConsent = "settings-tab-consent",
    /// `Fan-out and paging`
    SettingsTabFanOut = "settings-tab-fan-out",
    /// `Federation behaviour`
    SettingsTabFederation = "settings-tab-federation",
    /// `FHIR face`
    SettingsTabFhir = "settings-tab-fhir",
    /// `The running configuration could not be read from { $replica }.`
    SettingsUnreadable = "settings-unreadable",
    /// `{ $value } years`
    SettingsYears = "settings-years",
}
