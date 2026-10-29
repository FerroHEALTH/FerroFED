- The gateway reads its configuration with the family's loader
  (`ferrohealth-config` 0.2.8). Every subcommand takes `--set key=value`,
  repeatable, applied over the file and the `FERROFED__` overrides, and a
  reload of the file applies the same pairs again. A refusal names the
  dotted key and the variable or `--set` pair that set it, with the closest
  known key for a misspelt one, and quotes no value; a file that is not TOML
  is named by its line and column. The running configuration names a value
  a `--set` pair set as `set`. An override such as `FERROFED__LISTEN` with
  no section now names a top-level key, and is refused as unknown where it
  was refused for its shape.
