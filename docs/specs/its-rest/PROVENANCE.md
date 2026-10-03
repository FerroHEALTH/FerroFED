<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the openEHR ITS-REST OpenAPI documents

Vendored verbatim by `scripts/vendor/its-rest.sh`. Never edit a file here:
change the pin in docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/openEHR/specifications-ITS-REST>
- Pin: tag `Release-1.1.0`, which resolves to commit `24058992d5fa96e8dfbd855d9c133f328387fc09`
- Fetched: 2026-10-03
- Upstream licence: the specification content declares `Creative Commons Attribution-NoDerivs 3.0 Unported` in each
  document's `info.license`. The repository's own `LICENSE` file is the
  Apache License 2.0 and is vendored beside this file, so both statements are
  here and neither is assumed.
- Layout: the upstream paths, unchanged
- Files: 9
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `b45e39f27438c694aa44638f03bb31bcc43e169ba749ce35842465b765dc5714`
- Read by: #26 (the façade and the dispatch on the generated ITS-REST contract)
  and #310 (the result-set schema test against `query-validation.openapi.yaml`)

## Why every module

The Federation Tier with AQL specification binds this release by name and
makes the gateway transparent over the whole ITS-REST surface, read and
write, naming what it does not federate (§7a). The gateway therefore reads
every module: the ones it fans out or routes, and the ones it refuses. Each
module's lifecycle status, as its document declares it:

| Module | Title | `info.x-status` |
|---|---|---|
| overview | Overview | `STABLE` |
| system | System API | `STABLE` |
| ehr | EHR API | `STABLE` |
| query | Query API | `STABLE` |
| definition | Definition API | `STABLE` |
| demographic | Demographic API | `DEVELOPMENT` |
| admin | Admin API | `DEVELOPMENT` |

The `-codegen` rendering of each module is taken. The `-html` and
`-validation` renderings of the same release describe the same API, with one
exception taken here: `query-validation.openapi.yaml`, because Federation
Tier with AQL §9.1 names it, schema `ResultSet`, as the normative list of
the RESULT_SET members that the result-set schema's `$defs/itsRest` subset
restates. The Simplified Formats sources are not here because the gateway
passes a commit body through unmodified and never reads its format.
At this tag the Query API's validation and code-generation documents are the
same bytes: the table below gives both one git blob id.

## Why a blob id per file

Every document at this tag says `info.version: latest`, so the file content
carries no release identity of its own. The tag, the commit it resolves to,
and the git blob id of each file are what identify these bytes.

| File | sha256 | git blob id |
|---|---|---|
| `LICENSE` | `c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4` | `261eeb9e9f8b2b4b0d119366dda99c6fd7d35c64` |
| `computable/OAS/admin-codegen.openapi.yaml` | `77a5740d4ee6432bc3d0ae194b44c6a9531adfd4932dd66dd952cc15c2e45a0d` | `64f957f67ba27e5326f31c6edfa2e3fe077000ca` |
| `computable/OAS/definition-codegen.openapi.yaml` | `6c30fe7552ee7fea57ae97e00137f863a0b28a733c27dc4ed97c5ea9bacae930` | `28fec040ff586e0e425bf6c1303716b4c638bb7f` |
| `computable/OAS/demographic-codegen.openapi.yaml` | `7c61717aaa68ad1dc93f0cf37ec4ced239ef61ab2a64e1f0ce71e88248e2c641` | `373bea36611dc279b48e40e35621f388e8f659bd` |
| `computable/OAS/ehr-codegen.openapi.yaml` | `a0e37a217524c5a2c6351d128041c88d1c137fcde106badda05dde8c0269cd5c` | `d18ba6bbb0ac503a62840c0e83d8fdfbb72bf415` |
| `computable/OAS/overview-codegen.openapi.yaml` | `67761b4b06d6439a146ae04857520dad4bf1bacf448380d265dfff282a7788b9` | `79bd302402949f1b364e0d0f3b3adb3b2ace96ec` |
| `computable/OAS/query-codegen.openapi.yaml` | `d92e82c9cd6c9c8f6543ea425ea88b11e2fd0b133a1003347d470625d1d19bec` | `0a56228f763f1306a85a1edf4258a3a8a1d07757` |
| `computable/OAS/query-validation.openapi.yaml` | `d92e82c9cd6c9c8f6543ea425ea88b11e2fd0b133a1003347d470625d1d19bec` | `0a56228f763f1306a85a1edf4258a3a8a1d07757` |
| `computable/OAS/system-codegen.openapi.yaml` | `82c8a6583d833da395f7add0961c27f3796c32a55256a01b54e541ad232d4213` | `e815cbe028eaa70ebf21e9536fe657209a5b3bc4` |
