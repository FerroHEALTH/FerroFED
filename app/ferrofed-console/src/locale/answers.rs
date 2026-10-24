// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The messages of `locales/en/answers.ftl`.

messages! {
    /// `{ $minutes }:{ $seconds } left`
    CountdownLeft = "answer-countdown-left",
    /// `the sign-in could not be completed at the OpenID Provider`
    AnswerExchangeFailed = "answer-exchange-failed",
    /// `the OpenID Provider's ID Token did not verify`
    AnswerIdTokenUnverified = "answer-id-token-unverified",
    /// `the redirect carries no code`
    AnswerNoCode = "answer-no-code",
    /// `the post names no preference`
    AnswerNoPreference = "answer-no-preference",
    /// `no sign-in is pending`
    AnswerNoSignInPending = "answer-no-sign-in-pending",
    /// `a post keeps one preference`
    AnswerOnePreference = "answer-one-preference",
    /// `the console takes this request only from its own pages`
    AnswerOwnPagesOnly = "answer-own-pages-only",
    /// `the preference or its value is not one the console offers`
    AnswerPreferenceUnknown = "answer-preference-unknown",
    /// `the OpenID Provider did not sign the operator in`
    AnswerProviderRefused = "answer-provider-refused",
    /// `the session could not be begun`
    AnswerSessionNotBegun = "answer-session-not-begun",
    /// `the session could not be ended`
    AnswerSessionNotEnded = "answer-session-not-ended",
    /// `the console holds as many sessions as it may; try again later`
    AnswerSessionsFull = "answer-sessions-full",
    /// `the sidebar is not rail or full`
    AnswerSidebarUnknown = "answer-sidebar-unknown",
    /// `sign-in is not configured on this console`
    AnswerSignInNotConfigured = "answer-sign-in-not-configured",
    /// `the sign-in redirect could not be written`
    AnswerSignInRedirectUnwritten = "answer-sign-in-redirect-unwritten",
    /// `a sign-in session could not be begun`
    AnswerSignInSessionNotBegun = "answer-sign-in-session-not-begun",
    /// `the sign-out redirect could not be written`
    AnswerSignOutRedirectUnwritten = "answer-sign-out-redirect-unwritten",
    /// `the sign-in state does not match this session`
    AnswerStateMismatch = "answer-state-mismatch",
}
