- The access record of a routed openEHR read or write names the object it
  reached as `AuditEvent.entity[data].what.identifier` (the openEHR path,
  such as `/ehr/<ehr_id>/composition/<uid>`, with no `system`), where it
  wrote that path as `what.reference`, which is no FHIR literal reference
  (FHIR R4 references.html §2.3.0.2, §2.3.0.3); an Audit Record Repository
  that checks references, HAPI FHIR among them, refused those records. A read
  on the FHIR face keeps `what.reference` (`MedicationRequest/<id>`). This
  comes with FerroAUDIT `ihe-atna` and `ehds-logging` 0.3.2.
