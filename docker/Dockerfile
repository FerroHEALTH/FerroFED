# syntax=docker/dockerfile:1
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# The FerroFED image: one static binary on distroless static, per platform.
# Nothing is compiled here. The release lane builds the musl binaries and
# stages them under dist/<os>_<arch>/ferrofed before this file is built for
# linux/amd64 and linux/arm64 (docs/release.md); locally,
# scripts/release/stage-dist.sh stages the binaries of a published release.
# .dockerignore holds the build context to that one directory.
#
# Build from a staged dist/ tree, at the repository root:
#   docker buildx build -f docker/Dockerfile --platform linux/arm64 .

# The ITI-20 audit spool's directories (#442), made in an empty stage because
# distroless has no shell to make them: /var/lib/ferrofed and the spool path
# under it. The final stage copies them owned by the gateway user with mode
# 0700, so a named volume mounted at /var/lib/ferrofed starts with that owner
# and mode and needs no step on the host. The spool holds audit records that
# name patients, so the volume belongs on an encrypted disk.
FROM scratch AS spool
WORKDIR /spool/ferrofed/audit-spool

# The image the gateway runs on, the last stage: what versions.sh holds to
# docs/VERSIONS.md and release-compose.sh runs `config check` in.
#
# distroless static: ca-certificates, tzdata, an /etc/passwd carrying the
# nonroot user (uid 65532), a /tmp, no shell and no package manager. Pinned by
# the digest of its image index, which is what Docker resolves for a
# multi-platform build; docs/VERSIONS.md carries the pin and
# scripts/checks/versions.sh holds the two equal.
# https://github.com/GoogleContainerTools/distroless
FROM gcr.io/distroless/static-debian13:nonroot@sha256:e2e927ec666bae08560abb3c55d0659eceabb657f56b6782ab500a9fc7f555e3

# The platform of the manifest being built, set by buildx per platform.
# https://docs.docker.com/reference/dockerfile/#automatic-platform-args-in-the-global-scope
ARG TARGETOS
ARG TARGETARCH

# Root-owned, executable, and not writable by the runtime user.
COPY --chmod=0555 dist/${TARGETOS}_${TARGETARCH}/ferrofed /usr/local/bin/ferrofed

# The audit spool's directories, /var/lib/ferrofed/audit-spool, owned by the
# runtime user and closed to everyone else: the gateway refuses a spool its
# group or other users can enter.
COPY --from=spool --chown=65532:65532 --chmod=0700 /spool/ /var/lib/

# The binary's own default is loopback, which no container can publish, so the
# image default is every interface on the documented port. The configuration
# key is `[server] listen`; every override is FERROFED__<SECTION>__<KEY>.
ENV FERROFED__SERVER__LISTEN=0.0.0.0:8080
EXPOSE 8080/tcp

# Numeric on purpose: the kubelet cannot resolve a user name against an image
# it does not read, so `runAsNonRoot: true` refuses a named USER.
USER 65532:65532

# The base has no shell and no HTTP client, so the healthcheck is the binary's
# own subcommand: it asks GET /health/readiness on the configured listen port
# over loopback and exits 0 on 200, 1 otherwise. Readiness gates only on the
# gateway's own subsystems, never on a member node.
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD ["/usr/local/bin/ferrofed", "healthcheck"]

# Exec form: the binary is PID 1 and receives SIGTERM itself, which it drains
# on.
ENTRYPOINT ["/usr/local/bin/ferrofed"]
CMD ["serve"]

# Build-independent labels only. The release lane sets the version, revision
# and created labels at build time.
LABEL org.opencontainers.image.title="FerroFED" \
      org.opencontainers.image.description="An openEHR federation gateway: one AQL query in, standard AQL to every node, one answer out" \
      org.opencontainers.image.url="https://ferrofed.eu" \
      org.opencontainers.image.documentation="https://ferrofed.eu/docs/operate/container.html" \
      org.opencontainers.image.source="https://github.com/FerroHEALTH/FerroFED" \
      org.opencontainers.image.vendor="FerroHEALTH" \
      org.opencontainers.image.authors="FerroHEALTH <https://github.com/FerroHEALTH>" \
      org.opencontainers.image.licenses="BUSL-1.1" \
      org.opencontainers.image.base.name="gcr.io/distroless/static-debian13:nonroot"
