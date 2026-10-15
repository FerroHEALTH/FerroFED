- A stored configuration revision that does not read is logged by its
  position, key and kind of fault, never by a line of the revision the TOML
  or JSON reader quotes, so a credential in a URL query never reaches the
  log. A configuration parse refusal now uses the same fault words as a
  registry document refusal: `unknown field`, `duplicate key`, `invalid
  type, expected …`.
