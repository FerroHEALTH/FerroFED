// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The messages of the query console.

/// Appends these messages to the declaration `$acc`, then hands it to the
/// next macro of the chain `[$next, $rest...]`.
macro_rules! query_messages {
    ([$next:ident $(, $rest:ident)*] $($acc:tt)*) => {
        $next! {
            [$($rest),*]
            $($acc)*
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
            /// `Run AQL or a stored query across the federation.`
            RunAqlStoredQueryAcrossFederation = "run-aql-stored-query-across-federation",
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
            /// A node that refused.
            StateRefused = "state-refused",
        }
    };
}
