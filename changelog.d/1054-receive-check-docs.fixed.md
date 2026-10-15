- The documentation of the receive check in `eehrxf` 0.0.23 says what the
  code does (#1054): the `required` terminology bindings are held by
  `ReceivedDocument::check_bindings` and `terminology::bindings::check_required`,
  whose results convert into the check's findings, and `Category::document_type`
  and `Category::profiles` give `None` for a prescription or a dispensation in
  every build, because HL7 Europe MPD 1.0.0 has no document profile.
