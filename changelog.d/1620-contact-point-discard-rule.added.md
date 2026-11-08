- A national contact point's dispensation and its discard follow the
  MyHealth@EU rules. A token that lists permissions without `PPD-046` is
  refused `permission-missing` with `ERROR_SEC_GENERIC` on a dispensation
  and `discard-refused` with `ERROR_ED_DISCARD_FAILED` on a discard (eHDSI
  SAML Profile 10.0.0 §2.5). Each dispensation the owner creates is kept
  for 24 hours in FerroFED's store, in the new table
  `ferrofed.contact_point_dispensation`, with the identity assertion's
  `organization-id` and the facility identifier on `performer.actor`. A
  discard is admitted only within those 24 hours and from that organisation
  and facility; otherwise it is refused `discard-refused` before the member
  is asked (Requirements Catalogue 10.0.0 07.04).
