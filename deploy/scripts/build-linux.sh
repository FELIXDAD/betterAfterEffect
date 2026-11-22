#!/bin/sh
# Build static (musl) bae-hub binaries for Linux NAS: x86_64 and aarch64.
#
#   deploy/scripts/build-linux.sh            # both architectures
#   deploy/scripts/build-linux.sh aarch64    # one
#
# Works on macOS and Linux with rustup; no C toolchain, zig or Docker needed:
# Rust's own linker (rust-lld) links the self-contained musl targets.
# Output: dist/bae-hub-<version>-<arch>-linux-musl.tar.gz (+ .sha256) in the
# repository root.
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"

arches=${*:-"x86_64 aarch64"}
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
toolchain=$(sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml)
# Wherever cargo builds (CARGO_TARGET_DIR, build.target-dir in a cargo config, ./target).
target_dir=$(cargo metadata --format-version 1 --no-deps | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
mkdir -p dist

sha256() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1"; else shasum -a 256 "$1"; fi
}

for arch in $arches; do
    case "$arch" in
        x86_64 | aarch64) ;;
        *) echo "unknown architecture: $arch (use x86_64 or aarch64)" >&2; exit 2 ;;
    esac
    triple="$arch-unknown-linux-musl"
    rustup target add --toolchain "$toolchain" "$triple" >/dev/null
    upper=$(echo "$triple" | tr 'a-z-' 'A-Z_')
    env "CARGO_TARGET_${upper}_LINKER=rust-lld" \
        cargo build --release --locked -p bae-hub --target "$triple"
    bin="$target_dir/$triple/release/bae-hub"
    name="bae-hub-$version-$arch-linux-musl"
    stage=$(mktemp -d)
    mkdir "$stage/$name"
    cp "$bin" "$stage/$name/bae-hub"
    cp deploy/systemd/bae-hub.service "$stage/$name/"
    tar -C "$stage" -czf "dist/$name.tar.gz" "$name"
    rm -rf "$stage"
    (cd dist && sha256 "$name.tar.gz" > "$name.tar.gz.sha256")
    size=$(wc -c < "$bin" | tr -d ' ')
    echo "$name: binary $size bytes -> dist/$name.tar.gz"
done
