- The document list (`GET {fhir-base}/DocumentReference`) names each
  member's laboratory reports when a `[[fhir.receive]]` entry receives
  `Laboratory-Reports`: one `EehrxfMhdDocumentReference` of the type
  `11502-2` per laboratory composition, read at a `Binary/{token}` URL the
  gateway signs, which writes the composition as an HL7 Europe Laboratory
  Report 2.0.0 `Bundle-eu-lab` through the laboratory mapping of `eehrxf`.
  A member that does not answer the laboratory query fails the list under
  all-or-nothing, and each list and read is recorded under
  `Laboratory-Reports`.
