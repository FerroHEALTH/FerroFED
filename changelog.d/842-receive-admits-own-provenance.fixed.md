- The receive path admits the patient summary the gateway writes: an entry
  of a type no slice names is admitted where the profile slices
  `Bundle.entry` open, as `bundle-eu-eps` does, and refused only where the
  slicing is closed, or open at the end and the entry comes before a sliced
  one; an untyped reference by identifier alone is admitted at
  `Provenance.entity.what`, where the summary names each source composition,
  and refused everywhere else as before. Such an entry stays in the kept
  original and becomes no openEHR content (#842).
