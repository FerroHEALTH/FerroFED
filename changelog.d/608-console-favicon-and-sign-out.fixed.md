- The operator console's sign-out reaches the provider's end-session
  endpoint in the browser: the Content-Security-Policy's `form-action` now
  names that endpoint's origin beside the console, where before the browser
  blocked the redirect the sign-out form is answered with (#608). The console
  also serves the brand favicon at `/favicon.ico`, which every browser asks
  for on its first page and which answered `404` before.
