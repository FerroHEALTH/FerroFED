<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the openEHR ITS-REST OpenAPI documents

Vendored verbatim by `scripts/vendor/its-rest.sh`. Never edit a file here:
change the pin in docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/openEHR/specifications-ITS-REST>
- Pin: tag `Release-1.1.0`, which resolves to commit `24058992d5fa96e8dfbd855d9c133f328387fc09`
- Fetched: 2026-10-04
- Upstream licence: the specification content declares `Creative Commons Attribution-NoDerivs 3.0 Unported` in each
  document's `info.license`. The SMART on openEHR source carries no licence
  line of its own: its `master.adoc` includes the openEHR front block
  (`docs/boilerplate/full_front_block.adoc` of
  <https://github.com/openEHR/specifications-AA_GLOBAL>), whose licence block
  states Creative Commons Attribution-NoDerivs 3.0 Unported
  (<https://creativecommons.org/licenses/by-nd/3.0/>), which permits verbatim
  redistribution with attribution. That front block is cited by URL on the
  default branch of `specifications-AA_GLOBAL`; it is neither vendored nor
  pinned here, because it is boilerplate the rendering includes and none of
  the gateway's citations read it. The repository's own `LICENSE` file is
  the Apache License 2.0 and is vendored beside this file, so every statement
  is here and none is assumed.
- Layout: the upstream paths, unchanged
- Files: 26
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `ce825a8981925ad4907713014a34a80c92f6356efd6631d69b12ff79ad4230c8`
- Read by: #26 (the façade and the dispatch on the generated ITS-REST contract),
  #310 (the result-set schema test against `query-validation.openapi.yaml`)
  and #414 (client authentication's citations of SMART on openEHR, held to
  the vendored headings and status by the server's citation test)

## SMART on openEHR

`docs/smart_app_launch/` is the AsciiDoc source of *SMART on openEHR (SMART)* at this release, whole:
the master document, its chapters (`master04-service_discovery.adoc`,
`master07-authorization.adoc` and `master08-scopes.adoc` among them), its
`manifest_vars.adoc` and its diagrams. Its `manifest_vars.adoc` declares
`:spec_status: DEVELOPMENT`: the document is in the DEVELOPMENT state
in this release, so what the gateway is held to by it is a draft the
release does not stabilise, and every citation of it says so. The rendered
`docs/smart_app_launch.html` is not taken: it is generated from this
source.

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
| `docs/smart_app_launch/diagrams/smart.drawio` | `92442feade65921d92cee26523836e2aeaa10af3091d18b0e67faa199ed54635` | `707848fa90ef9015b21b541336b7faffdfdccdff` |
| `docs/smart_app_launch/diagrams/smart_embedded_iframe_launch.svg` | `d230394a6f6ef4809bd0499d6655dba6209ae1cb9a78546d32cc4d72d2660a68` | `31ed5fb5f06636c12464e8e5a4c3c7b403597d50` |
| `docs/smart_app_launch/diagrams/smart_logo.svg` | `8ebef16461b85e13d986dd1c7e2f064a72226e17d0408fb824d696e0045089f0` | `b2272fb647246845048ed43e8a2b8dae4e75f7db` |
| `docs/smart_app_launch/diagrams/smart_service_discovery.svg` | `27abccef5dde6fa5c7465f02dbfd4abc64ae2570cecfc2243e86fc3f43903ee0` | `462829fc48dd8ad4b5e5f722ade13b141d5cc4c8` |
| `docs/smart_app_launch/diagrams/smart_standalone_launch.svg` | `7e93abfac6129aa1a163c3bafd622a4ed7614e2edab1e26d9f939b47a27f5488` | `5a3f8876867457a0059bec797f4b7f8d6091b439` |
| `docs/smart_app_launch/manifest_vars.adoc` | `74fad9a1193457424f98eff44c217bc9607efca4d37be5b68cc50376cd823c85` | `284226a900b8ed921bf88fb22eb6516c93daa0ac` |
| `docs/smart_app_launch/master.adoc` | `f87d3bdec2bd7b8c02553896f64290d70067a83ca32e9678171cfbc81079b607` | `5202c7c5d47c1d783b75aa7c106741314f7000c0` |
| `docs/smart_app_launch/master00-amendment_record.adoc` | `6e3e1fc9ce29e8bdd952c3fc2ef185d2ee2eed3b1a4fd43de6b3df97598779da` | `985c0adb4504f8a4ede80ded50facddf8814c546` |
| `docs/smart_app_launch/master01-preface.adoc` | `19cf79ef9a168551b890651a65b01ca343e1558ea0ae1f8a90b9bc1a4b0b75b3` | `e7283f7bf6e9ffab3a764874f006e1f4e49e0203` |
| `docs/smart_app_launch/master02-overview.adoc` | `6b112092dd891d2e0e7c535731a73c67ce70082a1a53b60014938306338ce8f1` | `53553b7c42c9325ff91ddd0a0a956dab1f7ebc77` |
| `docs/smart_app_launch/master03-registration.adoc` | `295e548735a1a9e3bbf5ed185fb3db0dcc6ffc67b98d9fe21b247f4bfe2b94f8` | `404a898d6f7a3aeb17d2dd7982aeb88995ab02b2` |
| `docs/smart_app_launch/master04-service_discovery.adoc` | `06337d1026b48786523c956c5db04ea6feed7fa00c0bd56b3a8150b82582b812` | `6d1f679bc1222bf57f14affb022ded509fa38553` |
| `docs/smart_app_launch/master05-application_types.adoc` | `398e56ef45de4d4fe27387f9b6e0404576696e4254a197e4d29999504812f8db` | `34c19636daa391549436cfcab023d2c727768ed9` |
| `docs/smart_app_launch/master06-authentication.adoc` | `79e9e6777c91026f5ec36f846610a355387c5f867b5552dde2b91094c59cf2dd` | `98876571ccd5de8faed6b8f800efabefd00cfceb` |
| `docs/smart_app_launch/master07-authorization.adoc` | `590ccbca7d2d29df89a0c01338f54cf67151c7f7478600203e2375f0e134d777` | `b4b104addc88182af850f50063d657ddf1d8a08b` |
| `docs/smart_app_launch/master08-scopes.adoc` | `46a6f65c13814c8a2d10bb428d59e6ef235bc33123ca23ce33c0a07461ae779f` | `cdb65d0e2fa65676ed2c877df7cf45da46db47f5` |
| `docs/smart_app_launch/master09-experimental_features.adoc` | `8568ca93a7f53c7ba9f7d456e2f4cb90f546a5cb268be49fbc45855285cdf106` | `da2a2c19fbd1b52b21cdb940ea6a35313c1922f0` |
