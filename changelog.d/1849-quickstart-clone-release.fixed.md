- The quick start in the README and the book clones the release tag
  (`git clone --branch v<version>`), so the configuration a reader gets is
  the one the gateway image `compose.yaml` pins reads. A clone of `main`
  paired `main`'s renamed configuration keys with the last released
  gateway, which refused them and never turned healthy
  ([#1849](https://github.com/FerroHEALTH/FerroFED/issues/1849)).
