# syntax=docker/dockerfile:1
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# The FerroFED Console image: one static binary on distroless static, per
# platform. The binary draws every page and serves FerroUI's stylesheet,
# themes and script modules itself. Nothing is compiled here. The release
# lane builds the musl binaries and stages them under dist/console/ before
# this file is built for linux/amd64 and linux/arm64
# (.github/workflows/release-console.yml). The root .dockerignore holds the
# build context to dist/.
#
# Build from a staged dist/console tree, at the repository root:
#   docker buildx build -f docker/console/Dockerfile --platform linux/arm64 .

# distroless static: ca-certificates, tzdata, an /etc/passwd carrying the
# nonroot user (uid 65532), a /tmp, no shell and no package manager. The same
# base and digest as the gateway image, which docs/VERSIONS.md pins and
# scripts/checks/versions.sh holds equal for both files.
# https://github.com/GoogleContainerTools/distroless
FROM gcr.io/distroless/static-debian13:nonroot@sha256:d62067879de890a1def9da0e7766b7d20810a5e5555cef8506b752437d7a3e88

# The platform of the manifest being built, set by buildx per platform.
# https://docs.docker.com/reference/dockerfile/#automatic-platform-args-in-the-global-scope
ARG TARGETOS
ARG TARGETARCH

# Root-owned, executable, and not writable by the runtime user.
COPY --chmod=0555 dist/console/${TARGETOS}_${TARGETARCH}/ferrofed-console /usr/local/bin/ferrofed-console

# The binary's own default is loopback, so the image listens on every
# interface on the documented port. Every override is
# FERROFED_CONSOLE__<SECTION>__<KEY>.
ENV FERROFED_CONSOLE__SERVER__LISTEN=0.0.0.0:3000
EXPOSE 3000/tcp

# Numeric on purpose: the kubelet cannot resolve a user name against an image
# it does not read, so `runAsNonRoot: true` refuses a named USER.
USER 65532:65532

# The base has no shell and no HTTP client, so the healthcheck is the binary's
# own subcommand: it asks GET /health on the configured listen port over
# loopback and exits 0 on 200, 1 otherwise.
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD ["/usr/local/bin/ferrofed-console", "healthcheck"]

# Exec form: the binary is PID 1 and receives SIGTERM itself.
ENTRYPOINT ["/usr/local/bin/ferrofed-console"]
CMD ["serve"]

# Build-independent labels only. The release lane sets the version, revision
# and created labels at build time, and eu.ferrofed.information-sheet and
# eu.ferrofed.instructions-for-use, the URLs of the release's two copies.
LABEL org.opencontainers.image.title="FerroFED Console" \
      org.opencontainers.image.description="The FerroFED Console: a web client of the gateway's public surface" \
      org.opencontainers.image.url="https://ferrofed.eu" \
      org.opencontainers.image.documentation="https://ferrofed.eu/docs/operate/operator-console.html" \
      org.opencontainers.image.source="https://github.com/FerroHEALTH/FerroFED" \
      org.opencontainers.image.vendor="Cadasto B.V." \
      org.opencontainers.image.authors="Cadasto B.V. <info@cadasto.com>" \
      eu.ferrofed.manufacturer="Cadasto B.V., Comeniusstraat 2d, 1817 MS Alkmaar, The Netherlands, info@cadasto.com" \
      org.opencontainers.image.licenses="BUSL-1.1" \
      org.opencontainers.image.base.name="gcr.io/distroless/static-debian13:nonroot"
