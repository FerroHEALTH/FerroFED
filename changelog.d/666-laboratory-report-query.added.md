- The gateway holds a stored query for the laboratory report,
  `eu.ferrofed.eehrxf::laboratory-report`, read-only beside the patient
  summary's section queries: every whole composition that contains an
  `openEHR-EHR-OBSERVATION.laboratory_test_result.v1`, the entry the openEHR
  CKM template 1013.26.408 (asset version 3, DRAFT) names. The template is
  vendored by `scripts/vendor/openehr-ckm.sh` with its status recorded (#666).
