// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The messages of `locales/en/describe.ftl`.

messages! {
    /// `Decomposable aggregates`
    DescribeAggregates = "describe-aggregates",
    /// `The aggregates the gateway can merge across nodes.`
    DescribeAggregatesLine = "describe-aggregates-line",
    /// `Answered by`
    DescribeAnsweredBy = "describe-answered-by",
    /// `The replica this page read; each replica answers for itself.`
    DescribeAnsweredByLine = "describe-answered-by-line",
    /// `ITS-REST areas`
    DescribeAreas = "describe-areas",
    /// `How each area of the openEHR REST API is served.`
    DescribeAreasIntro = "describe-areas-intro",
    /// `Best effort`
    DescribeBestEffort = "describe-best-effort",
    /// `Whether a client may take an answer some nodes did not give.`
    DescribeBestEffortLine = "describe-best-effort-line",
    /// `Capabilities`
    DescribeCapabilities = "describe-capabilities",
    /// `What the gateway offers a query.`
    DescribeCapabilitiesIntro = "describe-capabilities-intro",
    /// `Deduplication modes`
    DescribeDedup = "describe-dedup",
    /// `How the gateway can merge rows that two nodes both answer.`
    DescribeDedupLine = "describe-dedup-line",
    /// `Definition`
    DescribeDefinition = "describe-definition",
    /// `How the DEFINITION area is served.`
    DescribeDefinitionLine = "describe-definition-line",
    /// `Demographic`
    DescribeDemographic = "describe-demographic",
    /// `How the DEMOGRAPHIC area is served; it is never federated.`
    DescribeDemographicLine = "describe-demographic-line",
    /// `EHR`
    DescribeEhr = "describe-ehr",
    /// `How the EHR area is served.`
    DescribeEhrLine = "describe-ehr-line",
    /// `Endpoints`
    DescribeEndpoints = "describe-endpoints",
    /// `The member endpoints the OPTIONS document lists, with their …`
    DescribeEndpointsIntro = "describe-endpoints-intro",
    /// `AQL fan-out`
    DescribeFanOut = "describe-fan-out",
    /// `Whether an AQL query goes to many nodes.`
    DescribeFanOutLine = "describe-fan-out-line",
    /// `Federation`
    DescribeFederation = "describe-federation",
    /// `Federation`
    DescribeFederationId = "describe-federation-id",
    /// `The identifier OPTIONS declares for this federation.`
    DescribeFederationIdLine = "describe-federation-id-line",
    /// `Who the gateway is, as its OPTIONS document declares it.`
    DescribeFederationIntro = "describe-federation-intro",
    /// `JWK Set`
    DescribeJwks = "describe-jwks",
    /// `The keys a node checks the gateway's signatures with.`
    DescribeJwksLine = "describe-jwks-line",
    /// `Limits`
    DescribeLimits = "describe-limits",
    /// `The budgets, the paging and the aggregates clients read; they are …`
    DescribeLimitsIntro = "describe-limits-intro",
    /// `On a localization failure`
    DescribeLocalization = "describe-localization",
    /// `What the gateway does when it cannot find where a patient's …`
    DescribeLocalizationLine = "describe-localization-line",
    /// `Overall`
    DescribeOverall = "describe-overall",
    /// `How long the gateway waits for the whole answer.`
    DescribeOverallLine = "describe-overall-line",
    /// `Offset strategy`
    DescribePaging = "describe-paging",
    /// `How the gateway pages a merged answer.`
    DescribePagingLine = "describe-paging-line",
    /// `Per node`
    DescribePerNode = "describe-per-node",
    /// `How long the gateway waits for one node.`
    DescribePerNodeLine = "describe-per-node-line",
    /// `Timeout policy`
    DescribePolicy = "describe-policy",
    /// `What the gateway does when a node runs out of time.`
    DescribePolicyLine = "describe-policy-line",
    /// `Query`
    DescribeQuery = "describe-query",
    /// `How the QUERY area is served.`
    DescribeQueryLine = "describe-query-line",
    /// `Stored-query registry`
    DescribeRegistry = "describe-registry",
    /// `Whether the gateway holds stored queries itself.`
    DescribeRegistryLine = "describe-registry-line",
    /// `Specification`
    DescribeSpecification = "describe-specification",
    /// `The text this gateway implements.`
    DescribeSpecificationLine = "describe-specification-line",
    /// `Stored-query fan-out`
    DescribeStoredFanOut = "describe-stored-fan-out",
    /// `Whether a stored query goes to many nodes.`
    DescribeStoredFanOutLine = "describe-stored-fan-out-line",
    /// `What clients see when they ask the gateway what it is. The OPTIONS …`
    DescribeSubtitle = "describe-subtitle",
    /// `About`
    DescribeSubtitleAbout = "describe-subtitle-about",
    /// `Template upload fan-out`
    DescribeTemplateFanOut = "describe-template-fan-out",
    /// `Whether a template upload goes to many nodes.`
    DescribeTemplateFanOutLine = "describe-template-fan-out-line",
    /// `Self-description`
    DescribeTitle = "describe-title",
    /// `Manufacturer`
    DescribeManufacturer = "describe-manufacturer",
    /// `Who makes the gateway, as its OPTIONS document declares it.`
    DescribeManufacturerLine = "describe-manufacturer-line",
    /// `Manufacturer's address`
    DescribeManufacturerAddress = "describe-manufacturer-address",
    /// `The postal address the gateway declares for its manufacturer.`
    DescribeManufacturerAddressLine = "describe-manufacturer-address-line",
    /// `Manufacturer's contact`
    DescribeManufacturerContact = "describe-manufacturer-contact",
    /// `The single point of contact the gateway declares.`
    DescribeManufacturerContactLine = "describe-manufacturer-contact-line",
    /// `Manufacturer's website`
    DescribeManufacturerWebsite = "describe-manufacturer-website",
    /// `The website the gateway declares for its manufacturer.`
    DescribeManufacturerWebsiteLine = "describe-manufacturer-website-line",
    /// `Fixes until`
    DescribeFixesUntil = "describe-fixes-until",
    /// `The last day the gateway's release receives fixes.`
    DescribeFixesUntilLine = "describe-fixes-until-line",
    /// `Not declared`
    DescribeNotDeclared = "describe-not-declared",
    /// `Declared in a form the console cannot read`
    DescribeUnreadable = "describe-unreadable",
    /// `No end date`
    DescribeNoEndDate = "describe-no-end-date",
}
