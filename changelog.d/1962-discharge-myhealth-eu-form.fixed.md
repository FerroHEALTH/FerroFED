- The discharge report the face writes is in the MyHealth@EU Hospital
  Discharge Report 9.2.0 form: its author is the one the record names (the
  identified composer, else the health care facility), its language is the
  record's language-region tag once the terminology server of
  `[eehrxf.terminology]` answers that `eHDSILanguage` holds it, and a record
  that names neither, or a deployment with no terminology server, is
  refused with the reason (#1962).
