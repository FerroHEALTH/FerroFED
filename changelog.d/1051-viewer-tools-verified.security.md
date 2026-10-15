- The operator console's site build installs the Tailwind CSS CLI and
  wasm-opt at pinned releases, checks each against its pinned SHA-256
  before it runs, and refuses to build without them, so cargo-leptos never
  downloads an unchecked binary into the attested release lane (#1051).
