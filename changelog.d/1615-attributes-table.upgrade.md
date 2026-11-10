- Add an `[[auth.attributes]]` table with `mapping = "iua"` and
  `authorization_details_purposes = true` for every `persons` issuer whose
  callers declare a purpose of use; without it every such caller is refused
  `purpose-of-use-required`. Move a `[[trust.issuer]]
  professional_issuing_authority = "<claim>"` to that table's
  `fields."professional.issuing_authority_name" = [{ claim = "<claim>" }]`;
  the old key now refuses to boot (#1615).