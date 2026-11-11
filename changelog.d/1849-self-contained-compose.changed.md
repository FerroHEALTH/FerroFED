- The quickstart is one self-contained `compose.yaml`, attached to each
  release with its `.sha256sum`, and the README and the book download it in
  place of a checkout. Every file it read from the repository is inlined as
  a Compose config, copied onto volumes by two one-shot services so the
  gateway, FerroPIX and their databases keep read-only root filesystems; a
  development issuer service, whose network namespace the gateway shares,
  signs the caller's token; the reader writes the gateway's signing key with
  one `openssl` line; and an `ehr-seed` service seeds the four nodes with the
  reference implementation's demonstration data, fetched at a pinned commit
  and refused unless each file has its pinned SHA-256. The gateway-only
  compose file of the production guide is attached as `gateway-compose.yaml`
  ([#1849](https://github.com/FerroHEALTH/FerroFED/issues/1849)).
