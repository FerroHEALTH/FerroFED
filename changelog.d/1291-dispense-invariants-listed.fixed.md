- A received dispensation's answer named as not evaluated only the
  invariants of `MedicationDispense-eu-mpd`'s root element, and none of its
  `required` bindings, so the invariants of every other element it carries
  and every binding were passed in silence. Each invariant of an element the
  dispensation carries or a slice it fills, and each `required` binding of
  the profile and of `Medication-eu-mpd`, is now listed as a `warning` of
  type `not-supported` (FHIR R4 conformance-rules.html §2.1.0.6,
  terminologies.html §4.1.5.1, #1291).
