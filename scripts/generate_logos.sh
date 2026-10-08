#!/usr/bin/env bash
set -euo pipefail

SVG="${1:-static/w3mm_logo.svg}"
ICON_DIR="src-tauri/icons"
STATIC_DIR="static"

declare -A SIZES=(
  ["Square107x107Logo.png"]=107
  ["128x128@2x.png"]=256
  ["Square30x30Logo.png"]=30
  ["32x32.png"]=32
  ["StoreLogo.png"]=512
  ["icon.png"]=512
  ["128x128.png"]=128
  ["Square150x150Logo.png"]=150
  ["Square44x44Logo.png"]=44
  ["Square71x71Logo.png"]=71
  ["Square89x89Logo.png"]=89
  ["Square310x310Logo.png"]=310
  ["Square142x142Logo.png"]=142
  ["Square284x284Logo.png"]=284
)

echo "Generating logos from $SVG"

for file in "${!SIZES[@]}"; do
  size=${SIZES[$file]}
  out="$ICON_DIR/$file"
  echo "  $file ${size}x${size} -> $out"
  convert "$SVG" -resize "${size}x${size}" "$out"
done

# favicon
echo "  favicon.png 32x32 -> $STATIC_DIR/favicon.png"
convert "$SVG" -resize 32x32 "$STATIC_DIR/favicon.png"

echo "Done."
