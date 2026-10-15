- The operator console's site build installs the Tailwind CSS CLI and
  wasm-opt at pinned releases, checks each against its pinned SHA-256
  before it runs, and refuses to build unless the tools on PATH match those
  digests, so cargo-leptos never downloads or runs an unchecked binary in
  the attested release lane (#1051).
