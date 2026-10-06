- The access record writes each category as a coded value (#824): the six
  Art 14(1) categories carry the codes of HL7 Europe's
  `EEHRxFDocumentPriorityCategoryCS` (`hl7.fhir.eu.health-data-api`
  1.0.0-ballot), written `<system>|<code>` in the `ehds-category`,
  `ehds-category-basis` and `ehds-retention-ground` details, with a new
  `ehds-category-version` detail naming the code system's version. A
  national category is its Member State's code in that state's system.
  No category and unclassified stay states of the record, and the reasons
  an access is unclassified are a closed set. `crates/ehds-logging`
  0.0.12 carries the change: `Category::system`, `version` and `token`,
  `NationalCategory` in place of `NationalCode`, and `classify::Unreadable`
  in place of a free reason string. The four packages the codes are read
  against are vendored with their provenance.
