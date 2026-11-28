- The interoperability component moves to `eehrxf` 0.51 with
  `ferrobridge-term` 0.24.2. A received discharge or imaging report's
  composition names each element it does not carry, by FHIRPath, in its
  Comment, the same elements the feeder audit declares beside the original,
  so hazards I-21 and I-22 close at S4, P1. A received laboratory report's
  losses are declared in the feeder audit alone, and I-16 stays open.
  Transcoding for MyHealth@EU asks `ValueSet/$validate-code` whether a
  concept is in a binding's value set before it asks `$translate`.
