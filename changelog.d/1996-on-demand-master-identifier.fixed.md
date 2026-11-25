- The summary written on request is listed under one master identifier,
  derived from its stable id, so `GET {fhir-base}/DocumentReference/{id}`
  answers the entry the list named. As an IHE On-Demand Document Entry, it
  carries neither hash nor size, and each summary written from it takes its
  own identifier (ITI TF-1 §18.3.3.1, #1996).
