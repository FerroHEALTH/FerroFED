- `ihe-iti` comes from crates.io at `=0.0.40`, published by
  FerroHEALTH/FerroPIX, which owns the crate from here on; the local copy
  under `crates/` is gone. Its features and its behaviour are the ones the
  gateway already used: the PIXm, PDQm, mCSD, PMIR and XCPD clients, with
  `balp` for the FHIR profiles' audit records and `atna` for the ITI-55
  audit message, both written through `ihe-atna`.
