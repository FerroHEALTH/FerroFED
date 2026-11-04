- Moved to `ferrohealth-server` 0.4.3, `ferrohealth-session` 0.2.2,
  `ferrohealth-smart` 0.8.8, `ferrohealth-ui` 0.6.6, `ihe-iti` 0.0.74 and
  `nl-generic-functions` 0.0.41. The operator console draws its pages with
  FerroUI 0.6.6, and the book's console screenshots are taken again.
- The operator console's sign-out goes to the provider's
  `end_session_endpoint` only when the session it ends holds an ID Token
  (OpenID Connect RP-Initiated Logout 1.0 §2). A sign-out with no session
  left, or one after a Basic sign-in, goes back to `/`.
