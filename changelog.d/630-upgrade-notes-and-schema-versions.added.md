- Upgrade notes, a configuration compatibility policy, and a versioned
  stored-query store (#630). Each release's changelog opens with "Upgrade
  notes", saying what an operator must change: `changelog.d/` takes an
  `upgrade` fragment, and CI runs `config check` from the new build over the
  example configuration the last release attached, and fails when it is
  refused and no upgrade note is pending. A renamed key is read under its
  old name for at least one release, and `config check` and the start-up
  log warn about it, naming the release that will refuse it; setting the
  old and the new key together is refused. The `redb` and PostgreSQL
  stored-query stores record a schema version (`ferrofed_schema` and
  `ferrofed.schema_version`), migrate forward in order when the gateway
  starts, and refuse to start over a store a newer FerroFED migrated, saying
  so. The book has an Upgrading page, with the notes for v0.0.8 and v0.0.9
  and the compatibility policy, and a Rollback page.
