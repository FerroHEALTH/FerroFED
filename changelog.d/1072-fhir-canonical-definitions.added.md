- A FHIR R4 definition behind every `https://ferrofed.eu/fhir/` URL the
  registry document in FHIR form and the access log use (#1072): the
  `consent-refusal-code`, `payload-profile` and `ehr-id-system` extensions of
  `Endpoint`, the `connection-type` and `access-log-origin` code systems, and
  a `NamingSystem` for each of the five `sid/*` identifier systems. The
  documentation site serves each at its URL and beside it as `.json`, and
  the book's new page, The registry in FHIR form, says how a FHIR client
  reads them from a host that sets the content type by extension, and why
  the archetype-local code systems, one per archetype, do not resolve. Each
  definition is tested against the FHIR R4 4.0.1 pages that govern its
  resource type, now with the CodeSystem, NamingSystem and terminologies
  pages vendored by `scripts/vendor/fhir-r4.sh`, and tied to the code that
  reads it, and a test fails on any other `ferrofed.eu` FHIR URL without a
  published definition.
