# syntax=docker/dockerfile:1
#
# bae-hub image: one static binary on an empty base (FROM scratch), about 6 MB.
#
# Build from the repository root (see deploy/README.md):
#   docker build -f deploy/docker/Dockerfile -t bae-hub .
#   docker buildx build -f deploy/docker/Dockerfile --platform linux/amd64,linux/arm64 -t <registry>/bae-hub:<version> --push .
#
# The build stage runs on the build machine's own architecture and
# cross-compiles to the target's musl (static) triple with the linker that
# ships with Rust (rust-lld), so multi-arch builds need no QEMU and no C
# toolchain.

FROM --platform=$BUILDPLATFORM rust:1-slim-bookworm AS build
ARG TARGETARCH
# The workspace's pinned toolchain (rust-toolchain.toml). Setting it here
# instead of reading the file avoids installing the desktop targets listed there.
ARG RUST_TOOLCHAIN=1.100
ENV RUSTUP_TOOLCHAIN=${RUST_TOOLCHAIN} \
    CARGO_TERM_COLOR=never \
    CARGO_TARGET_DIR=/build \
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld \
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld
RUN case "${TARGETARCH}" in \
        amd64) echo x86_64-unknown-linux-musl ;; \
        arm64) echo aarch64-unknown-linux-musl ;; \
        *) echo "unsupported architecture ${TARGETARCH}" >&2; exit 1 ;; \
    esac > /rust-target \
    && rustup toolchain install "${RUST_TOOLCHAIN}" --profile minimal --target "$(cat /rust-target)"
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build,sharing=locked \
    cargo build --release --locked -p bae-hub --target "$(cat /rust-target)" \
    && cp "/build/$(cat /rust-target)/release/bae-hub" /bae-hub \
    && mkdir -p /empty-data

FROM scratch
COPY --from=build /bae-hub /bae-hub
COPY --from=build /empty-data /data
ENV BAE_HUB_DATA=/data \
    BAE_HUB_LISTEN=0.0.0.0:7341
EXPOSE 7341
VOLUME ["/data"]
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD ["/bae-hub", "healthcheck"]
ENTRYPOINT ["/bae-hub"]
CMD ["serve"]
