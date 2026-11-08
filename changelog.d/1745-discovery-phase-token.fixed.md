- A national contact point's discovery-phase token, issued from the HP
  Identity Assertion alone before the patient is identified (eHDSI SAML
  Profile 10.0.0 §4), is admitted. It reaches the identification lookup,
  `GET {fhir-base}/Patient?identifier=`, and a request for patient data
  under it is refused `403 trc-missing` with nothing sent. It was refused
  at admission as `contact-point-attributes-required` (#1745).
