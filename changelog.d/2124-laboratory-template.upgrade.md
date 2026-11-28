- The laboratory report mapping of `eehrxf` 0.52 maps over
  `ferrobridge_laborbericht.v1.0.0`, the `KDS_Laborbericht` template with
  the report composition's Comment added, which the crate ships as
  `ferrobridge_laborbericht.opt`. Point the laboratory `[[fhir.receive]]`
  table's `template` at it, and upload it to the member that stores received
  laboratory reports, before you upgrade.
