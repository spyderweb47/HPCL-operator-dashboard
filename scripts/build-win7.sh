#!/usr/bin/env bash
# Builds the Windows 7/8/8.1 installer from a Mac (see README "Windows 7, 8 and 8.1").
#   scripts/build-win7.sh        64-bit (x64)
#   scripts/build-win7.sh x86    32-bit
# Needs: nightly Rust with rust-src, cargo-xwin, brew llvm + nsis, and the matching WebView2 109
# fixed runtime extracted to src-tauri/Microsoft.WebView2.FixedVersionRuntime.109.0.1518.78.<arch>/
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:/opt/homebrew/opt/llvm/bin:$PATH"

ARCH=${1:-x64}
case "$ARCH" in
  x64) RUST_TARGET=x86_64-win7-windows-msvc; BUNDLE_TARGET=x86_64-pc-windows-msvc; CONF=tauri.win7.conf.json ;;
  x86) RUST_TARGET=i686-win7-windows-msvc;   BUNDLE_TARGET=i686-pc-windows-msvc;   CONF=tauri.win7-x86.conf.json ;;
  *) echo "Usage: $0 [x64|x86]"; exit 1 ;;
esac
export XWIN_ARCH=x86,x86_64,aarch64 # cargo-xwin: also fetch the 32-bit Windows libraries

RUNTIME=src-tauri/Microsoft.WebView2.FixedVersionRuntime.109.0.1518.78.$ARCH
[ -f "$RUNTIME/msedgewebview2.exe" ] || { echo "Missing $RUNTIME (see README)"; exit 1; }

npm run build
# Rust 1.78+ dropped Windows 7 from the normal targets; the win7 targets need nightly + build-std.
# crt-static: the C runtime goes inside the exe (Windows 7 may lack the Universal CRT update).
touch src-tauri/build.rs
(cd src-tauri && TAURI_CONFIG="$(cat $CONF)" STATIC_VCRUNTIME=false \
  RUSTFLAGS="-C target-feature=+crt-static" \
  cargo +nightly xwin build --release -Zbuild-std=std,panic_abort \
  --target $RUST_TARGET --features tauri/custom-protocol)

# The Tauri CLI doesn't accept the win7 target names, so let it package the exe under the regular one.
OUT=src-tauri/target/$BUNDLE_TARGET/release
mkdir -p "$OUT"
cp src-tauri/target/$RUST_TARGET/release/hpcl-operator.exe "$OUT/hpcl-operator.exe"
npx tauri bundle --target $BUNDLE_TARGET --bundles nsis --config src-tauri/$CONF

VERSION=$(node -p 'require("./package.json").version')
cp "$OUT/bundle/nsis/HPCL Operator_${VERSION}_${ARCH}-setup.exe" "$OUT/bundle/nsis/HPCL-Operator-${VERSION}-win7-${ARCH}-setup.exe"
# Don't leave the win7 exe where the normal Windows build would pick it up without relinking.
rm "$OUT/hpcl-operator.exe"
echo "Built $OUT/bundle/nsis/HPCL-Operator-${VERSION}-win7-${ARCH}-setup.exe"
