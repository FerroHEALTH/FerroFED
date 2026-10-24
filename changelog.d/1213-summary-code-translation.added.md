- The patient summary the FHIR face writes translates a code in no preferred
  code system through the terminology server `[eehrxf.terminology]` names:
  each `equivalent` or `equal` match of `ConceptMap/$translate` into a
  preferred system is added after the original coding, and a translation the
  server refuses or does not answer fails the summary. With no server
  configured, the code is kept as recorded, under Art 10(7) of the eHealth
  Network Guidelines on Patient Summary
  ([#1213](https://github.com/FerroHEALTH/FerroFED/issues/1213)).
