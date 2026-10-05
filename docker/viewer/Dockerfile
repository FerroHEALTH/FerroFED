# syntax=docker/dockerfile:1
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# The FerroFED operator console image: one static binary and its site bundle
# on distroless static, per platform. Nothing is compiled here. The release
# lane builds the musl binaries and the site bundle and stages them under
# dist/viewer/ before this file is built for linux/amd64 and linux/arm64
# (.github/workflows/release-viewer.yml). The root .dockerignore holds the
# build context to dist/.
#
# Build from a staged dist/viewer tree, at the repository root:
#   docker buildx build -f docker/viewer/Dockerfile --platform linux/arm64 .

# distroless static: ca-certificates, tzdata, an /etc/passwd carrying the
# nonroot user (uid 65532), a /tmp, no shell and no package manager. The same
# base and digest as the gateway image, which docs/VERSIONS.md pins and
# scripts/checks/versions.sh holds equal for both files.
# https://github.com/GoogleContainerTools/distroless
FROM gcr.io/distroless/static-debian13:nonroot@sha256:e2e927ec666bae08560abb3c55d0659eceabb657f56b6782ab500a9fc7f555e3

# The platform of the manifest being built, set by buildx per platform.
# https://docs.docker.com/reference/dockerfile/#automatic-platform-args-in-the-global-scope
ARG TARGETOS
ARG TARGETARCH

# Root-owned, executable, and not writable by the runtime user. The site
# bundle is architecture-independent and root-owned too, so the runtime user
# reads it and cannot change it.
COPY --chmod=0555 dist/viewer/${TARGETOS}_${TARGETARCH}/ferrofed-viewer /usr/local/bin/ferrofed-viewer
COPY dist/viewer/site/ /site/

# The binary's own defaults are loopback and a checkout's target/site, so the
# image listens on every interface on the documented port and serves the
# bundle it carries. Every override is FERROFED_VIEWER__<SECTION>__<KEY>.
ENV FERROFED_VIEWER__SERVER__LISTEN=0.0.0.0:3000 \
    FERROFED_VIEWER__SERVER__SITE_ROOT=/site
EXPOSE 3000/tcp

# Numeric on purpose: the kubelet cannot resolve a user name against an image
# it does not read, so `runAsNonRoot: true` refuses a named USER.
USER 65532:65532

# The base has no shell and no HTTP client, so the healthcheck is the binary's
# own subcommand: it asks GET /health on the configured listen port over
# loopback and exits 0 on 200, 1 otherwise.
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD ["/usr/local/bin/ferrofed-viewer", "healthcheck"]

# Exec form: the binary is PID 1 and receives SIGTERM itself.
ENTRYPOINT ["/usr/local/bin/ferrofed-viewer"]
CMD ["serve"]

# Build-independent labels only. The release lane sets the version, revision
# and created labels at build time.
LABEL org.opencontainers.image.title="FerroFED operator console" \
      org.opencontainers.image.description="The FerroFED operator console: a web client of the gateway's public surface" \
      org.opencontainers.image.url="https://ferrofed.eu" \
      org.opencontainers.image.documentation="https://ferrofed.eu/docs/operate/operator-console.html" \
      org.opencontainers.image.source="https://github.com/FerroHEALTH/FerroFED" \
      org.opencontainers.image.vendor="FerroHEALTH" \
      org.opencontainers.image.authors="FerroHEALTH <https://github.com/FerroHEALTH>" \
      org.opencontainers.image.licenses="BUSL-1.1" \
      org.opencontainers.image.base.name="gcr.io/distroless/static-debian13:nonroot"
