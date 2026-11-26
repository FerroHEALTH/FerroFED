- The discharge report the face writes is in the MyHealth@EU Hospital
  Discharge Report 9.2.0 form, and HL7's FHIR Validator finds it valid
  against `bundle-hdr-myhealtheu`: its author is the one the record names
  (the identified composer, else the health care facility), its language is
  the record's language-region tag once the terminology server of
  `[eehrxf.terminology]` answers that `eHDSILanguage` holds it, and each
  discharge medicine names when it was ordered and by whom. A record that
  names no author or language, or a deployment with no terminology server,
  is refused with the reason. Catalogue row 14.01 is met (`eehrxf` 0.45,
  #1962).
