- The changelog is written as fragments (#598). A pull request with a
  user-visible effect adds one file under `changelog.d/`, named
  `<issue>-<kebab-slug>.<section>.md` and holding its entry, instead of editing
  `CHANGELOG.md`, so two pull requests no longer conflict on one file. The
  release cut runs `scripts/release/changelog.sh --assemble <version> <date>`,
  which writes the fragments and the entries already under `[Unreleased]` into
  the new version's section and removes the fragments. The new `changelog`
  job of CI runs `changelog.sh --check` over every fragment, and the
  `changelog-guard` job fails a pull request that adds no fragment and leaves
  `CHANGELOG.md` untouched, unless it carries the `no-changelog` label.
