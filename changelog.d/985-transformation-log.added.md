- The log of each semantic transformation the FHIR face applies (#985;
  eHealth Network General Guidelines Release 3.1 Art 6(3)): the access
  record of a patient summary and of a received document carries one
  `ehds-transformation` entity per transformation, naming the direction
  (`provided` or `received`), the Art 14(1) category, the context mapping,
  its template, profile and mapping files, the package a received document
  was held to, the terminology releases and code system versions in force,
  and the elements transcoded, left untranscoded (eHN PS Art 10(7)) and
  dropped, each counted at its FHIR element path or the path a declared loss
  names. No value, code value or patient identifier is written.
  `eehrxf` 0.0.18 adds `transformation::Report`, `Mapping::files` and
  `Mapping::pin`; `ehds-logging` 0.0.23 adds `transformation::Transformation`
  and `AccessRecord::transformations`.
