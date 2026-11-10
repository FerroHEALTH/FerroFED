- The release asset `compose.yaml` is now the self-contained quickstart. The
  gateway-only compose file of the production guide, which carried that name,
  is attached as `gateway-compose.yaml`: an operator who downloads it by name
  fetches `gateway-compose.yaml` and saves it as `compose.yaml` beside
  `ferrofed.toml` and `registry.toml`, as Install and run shows
  ([#1849](https://github.com/FerroHEALTH/FerroFED/issues/1849)).
