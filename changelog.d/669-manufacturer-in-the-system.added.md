- The running system names its manufacturer, Cadasto B.V., with its postal
  address and its single point of contact, as Regulation (EU) 2025/327 Art
  30(1)(g) asks (#669): `GET {base}/` and `OPTIONS {base}/` carry a
  `manufacturer` object (`name`, `postal_address`, `email`, `website`), the
  startup banner and `ferrofed --version` print it, as does
  `ferrofed-viewer --version`, and every page of the operator console names
  it in its footer. Both container images carry it in the
  `org.opencontainers.image.vendor` and `.authors` labels and a new
  `eu.ferrofed.manufacturer` label. The details are written once, and a test
  holds every surface and `LICENSE` to them.
