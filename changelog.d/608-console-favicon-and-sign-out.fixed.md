- The operator console's sign-out reaches the provider's end-session
  endpoint in the browser: the Content-Security-Policy's `form-action` now
  names that endpoint's origin beside the console, where before the browser
  blocked the redirect the sign-out form is answered with (#608). The console
  also serves the brand favicon at `/favicon.ico`, which every browser asks
  for on its first page and which answered `404` before.
- Every operator console page the gateway fills is whole in the HTML the
  server sends, so it reads with no script: the pages render once every
  answer is in, where before a page could arrive with its loading notice and
  the content in a template that only a script moved into place (#608). A
  query or a view the gateway refuses, and a query form the console cannot
  send, are shown with the gateway's status and code or the field at fault
  as before, and the browser no longer logs them as a failed request.
