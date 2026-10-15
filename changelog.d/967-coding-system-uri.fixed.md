- The interoperability component no longer writes an openEHR terminology id
  such as `local` as a FHIR `Coding.system` (#967). An archetype-local code
  is written under the code system of the archetype it is defined in,
  `https://ferrofed.eu/fhir/CodeSystem/archetype-local/` followed by the
  archetype id; `SNOMED-CT`, `LOINC` and `ICD10` are written as their FHIR
  URIs; and a code of a terminology with no known URI, or a local code with
  no template context, is written as no coding: its code is in the
  narrative, the result names it as unmapped, and the item follows eHN
  Patient Summary Art 10(7).
