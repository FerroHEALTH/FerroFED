- The FHIR face looks a patient up by identifier, `GET {fhir-base}/Patient?identifier=system|value`, as Xt-EHR D5.1
  §3.4.6 asks of a producer EHR system: the patient is resolved at the gateway and no member is asked, the answer is
  a `searchset` with the `Patient` the PDQm Supplier holds when a member holds the patient, an identifier no member
  holds and a restricted patient both answer the empty `searchset`, a search by demographics is refused, a lookup
  that finds the patient is recorded in the access log, and `metadata` declares the search (#1294).
