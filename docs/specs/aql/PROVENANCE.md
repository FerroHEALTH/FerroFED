<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the openEHR AQL specification source

Vendored verbatim by `scripts/vendor/aql.sh`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/openEHR/specifications-QUERY>
- Pin: tag `Release-1.1.0`, which resolves to commit `b03c48000d17eeae1e4f8868bfba3dfb59df8b3f`; the
  `manifest.json` of that commit dates release `1.1.0` `2021-05-14`
  and declares the AQL document `STABLE`
- Fetched: 2026-10-01
- Upstream licence: Creative Commons Attribution-ShareAlike 3.0 Unported, the
  repository's `LICENSE` file, vendored beside this file
- Layout: the upstream paths, unchanged
- Files: 27
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `a800d8631d179c243679ccdbc3d1adb6570ecf5828fccbef8a4c20990ad87f89`
- Read by: #18 (the rewrite on the AQL grammar and examples)

## What is here

- `docs/AQL/`: the AQL specification, one AsciiDoc file per chapter, with
  its figures.
- `docs/AQL/grammar/`: the ANTLR 4 grammar the specification publishes as
  its syntax reference.
- `docs/AQL_examples/`: the AQL examples document.
- `docs/index.adoc`, `manifest.json`, `README.adoc`: the component index
  and its release manifest.

The rendered `.html` pages of the release are left out; they are built from
these sources.

| Grammar file | sha256 |
|---|---|
| `docs/AQL/grammar/AqlLexer.g4` | `cd18ed42e7e9b4bd5bd44138562327267f163a18d3bb18d8c7807e65151fc472` |
| `docs/AQL/grammar/AqlParser.g4` | `bf6f1f59bc75bb0cc91212ca3c24dcfec047017eaad5ae3d06ec471b159e4256` |
