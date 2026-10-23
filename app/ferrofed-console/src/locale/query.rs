// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The messages of `locales/en/query.ftl`.

messages! {
    /// `The answer`
    QueryAnswerTitle = "query-answer-title",
    /// `The query is posted with this page and never reaches an address or …`
    QueryAqlHelp = "query-aql-help",
    /// `AQL`
    QueryAqlLabel = "query-aql-label",
    /// `Run a query to see its answer here.`
    QueryBeforeRun = "query-before-run",
    /// `Deduplication`
    QueryDedup = "query-dedup",
    /// `The gateway's default`
    QueryDedupDefault = "query-dedup-default",
    /// `Endpoints`
    QueryEndpoints = "query-endpoints",
    /// `Ask these endpoints only; with none chosen the gateway asks every …`
    QueryEndpointsHelp = "query-endpoints-help",
    /// `Fetch`
    QueryFetch = "query-fetch",
    /// `A chosen endpoint, organisation or mode cannot be sent as a header.`
    QueryHeaderIllegal = "query-header-illegal",
    /// `The qualified name, such as org.example::vital_signs.`
    QueryNameHelp = "query-name-help",
    /// `Stored query`
    QueryNameLabel = "query-name-label",
    /// `Name the stored query to run.`
    QueryNameStored = "query-name-stored",
    /// `Every asked endpoint answered and none holds a row.`
    QueryNoRows = "query-no-rows",
    /// `The answer holds no rows.`
    QueryNoRowsFailed = "query-no-rows-failed",
    /// `Offset`
    QueryOffset = "query-offset",
    /// `{ $count ->`
    QueryOptionsChosen = "query-options-chosen",
    /// `Federation options: every endpoint in service`
    QueryOptionsEvery = "query-options-every",
    /// `Organisations`
    QueryOrganisations = "query-organisations",
    /// `Add a parameter`
    QueryParameterAdd = "query-parameter-add",
    /// `Parameter … needs a name of letters, digits and underscores.`
    QueryParameterName = "query-parameter-name",
    /// `Name`
    QueryParameterNameLabel = "query-parameter-name-label",
    /// `Remove this parameter`
    QueryParameterRemove = "query-parameter-remove",
    /// `Parameter … names a parameter given before.`
    QueryParameterTwice = "query-parameter-twice",
    /// `Value`
    QueryParameterValueLabel = "query-parameter-value-label",
    /// `Parameters`
    QueryParameters = "query-parameters",
    /// `Each parameter the query names, such as $ehr_id, with its value.`
    QueryParametersHelp = "query-parameters-help",
    /// `No parameters.`
    QueryParametersNone = "query-parameters-none",
    /// `Take an incomplete answer (best effort)`
    QueryPartial = "query-partial",
    /// `Raw answer`
    QueryRaw = "query-raw",
    /// `The console cannot print this part of the answer.`
    QueryRawUnprintable = "query-raw-unprintable",
    /// `Rows`
    QueryRows = "query-rows",
    /// `Run`
    QueryRun = "query-run",
    /// `Ask the federation as yourself; every endpoint's part is shown …`
    QuerySubtitle = "query-subtitle",
    /// `AQL`
    QueryTabAql = "query-tab-aql",
    /// `Stored query`
    QueryTabStored = "query-tab-stored",
    /// `Query console`
    QueryTitle = "query-title",
    /// `Leave empty for the newest version.`
    QueryVersionHelp = "query-version-help",
    /// `Version`
    QueryVersionLabel = "query-version-label",
    /// `… is a whole number from 0.`
    QueryWholeNumber = "query-whole-number",
    /// `{ $count ->`
    QueryWindow = "query-window",
    /// `Write the AQL query to run.`
    QueryWriteAql = "query-write-aql",
}
