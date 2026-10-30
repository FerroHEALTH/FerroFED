- The operator console's site bundle (its WebAssembly, the `/pkg/` route
  that served it compressed, and the bundle budget), the
  `scripts/release/console-site.sh`, `console-tools.sh`,
  `scripts/checks/console-bundle.sh` and `console-paths.sh` scripts, and the
  Leptos, cargo-leptos, wasm-bindgen, wasm-opt and leptosfmt pins (#1457).
  A deployment that built the console itself runs `cargo build --release
  -p ferrofed-console` alone.
