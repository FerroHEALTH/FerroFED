- The operator console draws every page whole on its server with FerroUI
  0.6.0 and runs no WebAssembly (#1457). A link loads its page as one
  document, a form posts to the page it sits on and answers there, and
  FerroUI's small script modules add the menus, the dialogs, the row
  editing, the find's suggestions and the commit-confirm countdown on top;
  every page reads and every action posts with scripts turned off. The page
  no longer flickers or loses what was open when you act on it, and the
  404 page no longer panics in the browser (#1458). The console's
  Content-Security-Policy admits scripts and stylesheets from its own
  origin alone (`script-src 'self'; style-src 'self'`, no nonce and no
  `'wasm-unsafe-eval'`), and FerroUI's stylesheet, theme and modules are
  served under `/ferrohealth-ui/0.6.0/`, cached for a year, in place of
  `/ferrohealth-ui.css`, `/ferrofed.css` and the site bundle under `/pkg/`.
  The console image carries the binary alone.
