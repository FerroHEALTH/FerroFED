- `ferrofed serve` draws its startup banner with the family's
  `ferrohealth_server::build_info::banner`: the wordmark, the build line
  `ferrofed <version>`, and one aligned line per fact. The tagline, the
  maintainer, the manufacturer and its postal address are now the lines
  `Product`, `Maintainer`, `Manufacturer` and `Address`, and the facts are
  no longer split into groups by blank lines. The development notice and
  the support notification still follow the banner, in red on a terminal
  with colour (#1751).
