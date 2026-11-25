- Give the FHIR face a key for its stable ids: `fhir.id_key`, or
  `fhir.id_key_file`, 32 random bytes in base64url (for example
  `openssl rand 32 | basenc --base64url`). A configuration with `[fhir]` and
  no key is refused at start and by `config check`. Rotating the key changes
  every patient id the face gives
  ([Looking a patient up](https://ferrofed.eu/docs/integrate/patient-summary.html#looking-a-patient-up),
  #1976).
