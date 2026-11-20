#!/bin/sh
# Build the bae-hub Docker image (needs Docker with buildx).
#
#   deploy/scripts/build-image.sh                       # this machine's architecture, loaded into Docker
#   deploy/scripts/build-image.sh --tar amd64           # dist/bae-hub-<version>-amd64.tar to import on a NAS
#   deploy/scripts/build-image.sh --tar arm64
#   deploy/scripts/build-image.sh --push <registry>/bae-hub   # amd64 + arm64, pushed
#
# The build cross-compiles inside the build stage (no QEMU), so arm64 images
# build as fast as amd64 ones.
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
dockerfile=deploy/docker/Dockerfile

case "${1:-}" in
    "")
        docker buildx build -f "$dockerfile" --load -t "bae-hub:$version" -t bae-hub:latest .
        ;;
    --tar)
        arch=${2:?usage: build-image.sh --tar amd64|arm64}
        mkdir -p dist
        out="dist/bae-hub-$version-$arch.tar"
        docker buildx build -f "$dockerfile" --platform "linux/$arch" \
            -t "bae-hub:$version" -t bae-hub:latest \
            --output "type=docker,dest=$out" .
        echo "image saved to $out (import it in the NAS's container manager, or: docker load -i $out)"
        ;;
    --push)
        repo=${2:?usage: build-image.sh --push <registry>/bae-hub}
        docker buildx build -f "$dockerfile" --platform linux/amd64,linux/arm64 \
            -t "$repo:$version" -t "$repo:latest" --push .
        ;;
    *)
        echo "usage: build-image.sh [--tar amd64|arm64 | --push <registry>/bae-hub]" >&2
        exit 2
        ;;
esac
