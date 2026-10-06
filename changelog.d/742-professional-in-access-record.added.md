- The access record names the person behind each access as client
  authentication verified them (Regulation (EU) 2025/327 Annex II 3.1 and
  3.2(b); #742). The BALP `agent:user` carries the professional's IHE IUA
  `subject_name` as `who.display` and `national_provider_identifier` as an
  `ihe-otherId` extension typed `NPI`, the assurance level of the
  authentication as an `ihe-assuranceLevel` extension, and who acts,
  `person` or `client`, as its `role`. The level is recorded only when the
  issuer's `[auth.issuer.assurance]` table maps the value the token states,
  never inferred. No other part of the record, and no log line, carries
  them.
- `crates/ehds-logging` 0.0.4: `Accessor` carries the acting mode, the
  assurance level and the professional's name and identifier, and the
  requester's professional number moves to `alt_id`. `crates/ihe-iti`
  0.0.30: a BALP `User` takes a name, a provider identifier, roles and an
  assurance level, written to the `agent:user` (#742).
