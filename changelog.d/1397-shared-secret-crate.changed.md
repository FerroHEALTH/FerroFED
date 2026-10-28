- The gateway and the operator console hold every credential in the
  secret types of `ferrohealth-secret` 0.2.0, the FerroHEALTH family crate,
  and read every `<key>_file` sibling by its key-or-file rule; FerroFED's
  own copy in `ferrofed-registry` is deleted (#1397). A secret file now
  loses only its trailing line ending, where FerroFED trimmed white space
  at both ends: a file whose secret starts or ends with a space or a tab
  now passes it on as written, so a deployer whose secret files carry such
  white space by accident removes it before upgrading. The refusals read
  as the family words them: `<key> and <key>_file are both set; set exactly
  one`, `<key>_file names <path>, which could not be read` and
  `<key>_file names <path>, which is empty`.
