#!/bin/sh
# Checks of the C ABI header (sdk/c/bae_plugin.h) on macOS and Linux:
#   1. abi_check.c as C11 and C17 (struct layouts, constants, signatures);
#   2. the header as C++17 (syntax only);
#   3. the invert example built as a shared library and driven by mini_host.c.
# Usage: sdk/c/tests/check.sh [build-dir]   (CC / CXX override the compilers)
set -eu

here=$(cd "$(dirname "$0")" && pwd)
sdk=$(dirname "$here")
out=${1:-${TMPDIR:-/tmp}/bae-sdk-c-check}
cc=${CC:-cc}
cxx=${CXX:-c++}
mkdir -p "$out"

warn="-Wall -Wextra -Wpedantic -Werror"

for std in c11 c17; do
    "$cc" -std=$std $warn "$here/abi_check.c" -o "$out/abi_check_$std"
    "$out/abi_check_$std"
done

"$cxx" -std=c++17 -Wall -Wextra -Werror -x c++ -fsyntax-only "$sdk/bae_plugin.h"
echo "bae_plugin.h: OK as C++17"

case "$(uname -s)" in
    Darwin) lib="$out/libinvert.dylib"; shared="-dynamiclib" ;;
    *) lib="$out/libinvert.so"; shared="-shared -fPIC" ;;
esac
"$cc" -std=c11 $warn -fvisibility=hidden $shared -I"$sdk" \
    "$sdk/examples/invert/invert.c" -o "$lib"
"$cc" -std=c11 $warn "$here/mini_host.c" -o "$out/mini_host" -ldl 2>/dev/null ||
    "$cc" -std=c11 $warn "$here/mini_host.c" -o "$out/mini_host"

# Manifest values the host checks against the exports (spec 6.6, 7.2).
manifest="$sdk/examples/invert/bae-plugin.toml"
value() {
    sed -n "s/^$1 *= *\"\\([^\"]*\\)\".*/\\1/p" "$manifest" | head -n 1
}
"$out/mini_host" "$lib" "$(value id)" "$(value version)" "$(value api)" invert
