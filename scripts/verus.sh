#!/usr/bin/env bash
# The PTC checks of verus/ptc.rs: Verus proves the decision, compiles the checker, and the checker
# runs its self-test and then reads the Markdown files given (README.md when none). `just verus` and
# .github/workflows/verus.yml (run by hand) call it. Linux x86_64 only: the Verus release it pins.
#
# It needs rustup: Verus runs on the Rust toolchain it was built with (VERUS_TOOLCHAIN), which it
# installs. Verus itself goes to verus/.tools.
set -euo pipefail

# The weekly point release of 2026-11-02 (tag release/0.2026.11.02.8bf68ee) and its toolchain (the
# tag's rust-toolchain.toml).
VERUS_VERSION=0.2026.11.02.8bf68ee
VERUS_TOOLCHAIN=1.99.1
# ⚠ Not pinned yet: the release could not be downloaded when this was written. The script prints the
# sha256 of the zip it got; write it here.
VERUS_SHA256=""

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tools="$root/verus/.tools"
zip="verus-$VERUS_VERSION-x86-linux.zip"

die() {
    echo "verus: $*" >&2
    exit 1
}

[ "$(uname -s)-$(uname -m)" = Linux-x86_64 ] || die "only Linux x86_64 (the pinned release is x86-linux)"
command -v rustup >/dev/null 2>&1 || die "rustup not found (https://rustup.rs)"

if [ ! -x "$tools/$VERUS_VERSION/verus" ]; then
    rm -rf "${tools:?}/$VERUS_VERSION" "${tools:?}/unpack"
    mkdir -p "$tools/unpack"
    curl -fsSL --retry 3 -o "$tools/$zip" \
        "https://github.com/verus-lang/verus/releases/download/release/$VERUS_VERSION/$zip"
    sum="$(sha256sum "$tools/$zip" | cut -d' ' -f1)"
    echo "verus: $zip sha256 $sum"
    if [ -n "$VERUS_SHA256" ]; then
        [ "$sum" = "$VERUS_SHA256" ] || die "$zip: sha256 $sum, pinned $VERUS_SHA256"
    fi
    unzip -q "$tools/$zip" -d "$tools/unpack"
    bin="$(find "$tools/unpack" -type f -name verus -perm -u+x | head -n1)"
    [ -n "$bin" ] || die "$zip has no verus binary"
    mv "$(dirname "$bin")" "$tools/$VERUS_VERSION"
    rm -rf "${tools:?}/unpack" "${tools:?}/$zip"
fi

rustup toolchain install "$VERUS_TOOLCHAIN-x86_64-unknown-linux-gnu" --profile minimal --no-self-update

build="$tools/build"
mkdir -p "$build"
# Verifies every function inside verus! { } and, with --compile, builds the checker as $build/ptc.
(cd "$build" && "$tools/$VERUS_VERSION/verus" "$root/verus/ptc.rs" --compile)

"$build/ptc" --self-test
cd "$root"
if [ "$#" -eq 0 ]; then
    set -- README.md
fi
"$build/ptc" "$@"
