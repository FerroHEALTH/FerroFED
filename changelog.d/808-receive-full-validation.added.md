- A document received on the FHIR face is validated in full by
  FerroBRIDGE's `ferrobridge-validator`: every constraint of its category's
  profiles and of the profile each entry claims or its slice names, every
  invariant and terminology binding included. An error refuses the document
  with `422`, each warning is returned in the outcome of the write, and
  every constraint the validator did not evaluate is named there. A
  `required` binding the packages cannot decide is asked of the
  `[eehrxf.terminology]` server, and one it does not answer refuses the
  document with `502`. `scripts/vendor/fhir-packages.sh` fetches the
  packages of the Patient Summary's dependency closure that the repository
  does not commit (#808).
