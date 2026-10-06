- If you wrote an `[access_log]` map on a development build, rewrite its
  categories with the codes of HL7 Europe's
  `EEHRxFDocumentPriorityCategoryCS` (#824), which are case-sensitive:
  `patient-summary` becomes `Patient-Summaries`, `electronic-prescription`
  `Electronic-Prescriptions`, `electronic-dispensation`
  `Electronic-Dispensations`, `medical-imaging` `Medical-Imaging`,
  `medical-test-result` `Laboratory-Reports` and `discharge-report`
  `Discharge-Reports`, in `[access_log.templates]`,
  `[access_log.archetypes]` and the keys of
  `[access_log.retention.categories]`. Declare each national category in
  `national_categories` as `<system>|<code>`, the absolute URI of your
  Member State's code system (or one you control) and its code, and name it
  the same way in the map and the retention table. The old spellings and a
  national code without a system are refused when the configuration
  loads. A search of your Audit Record Repository for a category matches
  the `ehds-category` detail `<system>|<code>`, never the bare code.
