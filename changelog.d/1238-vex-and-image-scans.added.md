- OpenVEX statements for FerroFED under `security/vex/`: the Rust advisory
  the dependency gate accepts (RUSTSEC-2024-0436, `paste`) with its
  argument and dependency path, generated from `deny.toml`. Each release
  attaches `ferrofed-vX.Y.Z-vulnerability-record.json`, every finding of a
  full scan of both images and the binaries with the statement that judges
  it, and is refused while a finding has none; each image is scanned before
  it is tagged, and the published images are scanned every week with the
  VEX applied (#1238).
