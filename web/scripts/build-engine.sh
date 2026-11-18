#!/bin/sh
# Builds the Sheet engine (bindings/wasm) to WebAssembly into public/engine, where the
# editor loads it from its own package. Needs wasm-pack.
set -eu
web="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$web/.." && pwd)"
if toolchain="$(rustc --print sysroot 2>/dev/null)" && [ -d "$toolchain/lib" ]; then
  export DYLD_FALLBACK_LIBRARY_PATH="$toolchain/lib${DYLD_FALLBACK_LIBRARY_PATH:+:$DYLD_FALLBACK_LIBRARY_PATH}"
fi
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo/target}"
cd "$repo"
wasm-pack build bindings/wasm --target web --release --no-typescript --out-dir "$web/public/engine"
rm -f "$web/public/engine/.gitignore" "$web/public/engine/package.json"
# Chinese text needs a CJK face the web build does not embed: the editor gives the engine this one
# (from the kition-fonts checkout next to the repository, or KITION_FONTS_DIR) before it opens.
fonts="${KITION_FONTS_DIR:-$repo/../kition-fonts}/fonts/droid-sans-fallback"
rm -rf "$web/public/fonts"
if [ -f "$fonts/DroidSansFallbackFull.ttf" ]; then
  mkdir -p "$web/public/fonts"
  cp "$fonts/DroidSansFallbackFull.ttf" "$web/public/fonts/"
  cp "$fonts/NOTICE" "$web/public/fonts/DroidSansFallback-NOTICE.txt"
else
  echo "warning: no kition-fonts checkout at $fonts; Chinese text will show as boxes" >&2
fi
# An open Kai face for cells set in 楷体 (kition-fonts has no open Song or FangSong face).
kai="${KITION_FONTS_DIR:-$repo/../kition-fonts}/fonts/ma-shan-zheng"
if [ -f "$kai/MaShanZheng-Regular.ttf" ]; then
  mkdir -p "$web/public/fonts"
  cp "$kai/MaShanZheng-Regular.ttf" "$web/public/fonts/"
  cp "$kai/OFL.txt" "$web/public/fonts/ma-shan-zheng-OFL.txt"
else
  echo "warning: no Ma Shan Zheng in the kition-fonts checkout; 楷体 cells are drawn in the fallback face" >&2
fi
# Open, metric-compatible stand-ins for Calibri, Cambria, Arial, Times New Roman and Courier New
# (open decision F1): cell text set in those fonts takes the width it takes in Excel.
metric="${KITION_FONTS_DIR:-$repo/../kition-fonts}/fonts"
for f in carlito/Carlito-Regular carlito/Carlito-Bold carlito/Carlito-Italic carlito/Carlito-BoldItalic \
  caladea/Caladea-Regular caladea/Caladea-Bold caladea/Caladea-Italic caladea/Caladea-BoldItalic \
  arimo/Arimo-Variable arimo/Arimo-Italic-Variable tinos/Tinos-Regular tinos/Tinos-Bold \
  tinos/Tinos-Italic cousine/Cousine-Regular; do
  if [ -f "$metric/$f.ttf" ]; then
    mkdir -p "$web/public/fonts"
    cp "$metric/$f.ttf" "$web/public/fonts/"
    cp "$metric/${f%%/*}/OFL.txt" "$web/public/fonts/${f%%/*}-OFL.txt"
  else
    echo "warning: no $f in the kition-fonts checkout; its cells are drawn in the built-in face" >&2
  fi
done
