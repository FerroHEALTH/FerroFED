- `[fhir]` needs `[pdqm]`: configuration load refuses the FHIR face
  without a demographics binding, because the HL7 Europe Patient Summary
  `Patient` requires a name (`ips-pat-1`) and the members federate no
  demographics (#663).
