- Every release attaches the nginx reverse proxy configuration of the
  production guide as `ferrofed.conf`, with its SHA-256 as
  `ferrofed.conf.sha256sum`, and the release lane fails when it is missing
  (#725). The container page and the production guide download it from the
  release with the compose files.
