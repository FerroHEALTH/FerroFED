- The component maps the patient summary it writes back to openEHR (#862).
  The summary names its patient in one form, the relative reference
  `Patient/{id}`, in `Composition.subject` and in each mapped resource,
  where the absolute `fullUrl` in `Composition.subject` read as a second
  subject. Each mapped resource now claims in `meta.profile` the profile its
  FHIRconnect context maps to, once, so a receiver selects the mapping by it.
