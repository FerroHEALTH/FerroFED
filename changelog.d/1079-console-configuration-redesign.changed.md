- The operator console's configuration page is redesigned: five tabs, each
  at its own path (Members, Credentials and bindings, Stored queries,
  History, Import and export), with the card of the revision saved and not
  applied above each, its changes marked `−` and `+`, and its apply, a
  running revision's confirm and roll back, a removal and a stored version
  each confirmed in a step of their own instead of a checkbox. A member's
  endpoints, a credential's grant settings and a binding section's
  settings are rows of their own fields, never one line of separated or
  escaped text, and each secret is kept, set by a value, read from a file
  or cleared. A form's outcome shows at the top of its card with the focus
  moved there, a refused field is marked where it stands, and the history
  compares any revision with the running one (#1079). The Integrity page's
  collision clear and the stored queries' drift repair confirm in the same
  step.
