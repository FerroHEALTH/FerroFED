- The operator console serves its site bundle brotli- or gzip-compressed, as
  the browser's `Accept-Encoding` chooses, with `Vary: Accept-Encoding`, so a
  browser downloads about 220 KB of WebAssembly where it downloaded 770 KB;
  pages and server function answers stay uncompressed (#600). The release
  bundle names no directory of the host that built it: the build remaps the
  cargo home, the toolchain and the checkout, and CI fails a bundle that
  names a home, runner, registry or toolchain path.
