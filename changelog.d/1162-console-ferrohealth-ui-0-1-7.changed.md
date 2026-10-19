- The operator console draws with `ferrohealth-ui` 0.1.7 and is ready for
  more languages, English the one that ships (#1162). The library's words
  and the console's come from Fluent catalogs, the console's under
  `app/ferrofed-viewer/locales/`, so a language joins as a catalog file with
  no code change. The locale is chosen once per request, the best language
  of `Accept-Language` the console's catalog holds or else the new
  `[server] locale` (default `en`), and the page writes it as `<html lang>`.
  `scripts/checks/stray-literals.sh` fails CI on a console string outside
  the catalog beyond the per-file baseline the remaining strings hold while
  they move. The list rows of the configuration forms draw through the
  library's keyed field row, and the bundle checks refuse a site bundle the
  last release build did not write.
