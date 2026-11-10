- Each release attaches the quickstart as a bundle: `quickstart.tar.gz`,
  with its `.sha256sum`, holds the quickstart's `compose.yaml` with the
  gateway pinned by the release's digest, every file it mounts, the scripts
  it runs and the demonstration compositions the seed commits. CI builds
  the same bundle over the gateway it builds and runs the quickstart tour
  from the unpacked bundle
  ([#1849](https://github.com/FerroHEALTH/FerroFED/issues/1849)).
