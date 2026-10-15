- `ferrofed config check` names each Art 14(1) category the FHIR face
  receives, with the member its `[[fhir.receive]]` entry declares, and the
  categories no member receives: the set `GET {fhir-base}/metadata` names.
  A configuration with no `[fhir]` gets neither note (#930).
