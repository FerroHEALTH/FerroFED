- Two read-only stored queries in the reserved namespace,
  `eu.ferrofed.eehrxf::medication-orders` and
  `eu.ferrofed.eehrxf::medication-actions`: each patient's compositions that
  hold a medication order (imported copies left out) or a medication action,
  with each composition's uid and template, which the prescription face's
  map mode reads a member through.
