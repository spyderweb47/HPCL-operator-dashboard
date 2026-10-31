#!/usr/bin/env bash
# Builds the Windows 7/8/8.1 installer from a Mac (see README "Windows 7 build").
# Needs: nightly Rust with rust-src, cargo-xwin, brew llvm + nsis, and the WebView2 109
# fixed runtime extracted to src-tauri/Microsoft.WebView2.FixedVersionRuntime.109.0.1518.78.x64/
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:/opt/homebrew/opt/llvm/bin:$PATH"

RUNTIME=src-tauri/Microsoft.WebView2.FixedVersionRuntime.109.0.1518.78.x64
[ -f "$RUNTIME/msedgewebview2.exe" ] || { echo "Missing $RUNTIME (see README)"; exit 1; }

npm run build
# Rust 1.78+ dropped Windows 7 from the normal target; the win7 target needs nightly + build-std.
# crt-static: the C runtime goes inside the exe (Windows 7 may lack the Universal CRT update).
touch src-tauri/build.rs
(cd src-tauri && TAURI_CONFIG="$(cat tauri.win7.conf.json)" STATIC_VCRUNTIME=false \
  RUSTFLAGS="-C target-feature=+crt-static" \
  cargo +nightly xwin build --release -Zbuild-std=std,panic_abort \
  --target x86_64-win7-windows-msvc --features tauri/custom-protocol)

# The Tauri CLI doesn't accept the win7 target name, so let it package the exe as x86_64-pc-windows-msvc.
OUT=src-tauri/target/x86_64-pc-windows-msvc/release
mkdir -p "$OUT"
cp src-tauri/target/x86_64-win7-windows-msvc/release/hpcl-operator.exe "$OUT/hpcl-operator.exe"
npx tauri bundle --target x86_64-pc-windows-msvc --bundles nsis --config src-tauri/tauri.win7.conf.json

VERSION=$(node -p 'require("./package.json").version')
cp "$OUT/bundle/nsis/HPCL Operator_${VERSION}_x64-setup.exe" "$OUT/bundle/nsis/HPCL-Operator-${VERSION}-win7-x64-setup.exe"
# Don't leave the win7 exe where the normal Windows build would pick it up without relinking.
rm "$OUT/hpcl-operator.exe"
echo "Built $OUT/bundle/nsis/HPCL-Operator-${VERSION}-win7-x64-setup.exe"
