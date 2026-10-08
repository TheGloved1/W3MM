#!/usr/bin/env bash
# Regenerate every app logo from one SVG source (ported from NMM, extended
# to cover the icon.icns / icon.ico / android / ios sets that the NMM
# script leaves untouched).
#
# Usage: ./scripts/generate_logos.sh [path/to/logo.svg]
#
# Needs: ImageMagick (`magick` or `convert`) + python3 (icns packing only).
set -euo pipefail

SVG="${1:-static/w3mm_logo.svg}"
ICON_DIR="src-tauri/icons"
STATIC_DIR="static"

if command -v magick >/dev/null 2>&1; then
  IMG="magick"
else
  IMG="convert"
fi

# --- Tauri PNG set -------------------------------------------------------
declare -A SIZES=(
  ["Square107x107Logo.png"]=107
  ["128x128@2x.png"]=256
  ["Square30x30Logo.png"]=30
  ["32x32.png"]=32
  ["64x64.png"]=64
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
  echo "  $file ${size}x${size} -> $ICON_DIR/$file"
  "$IMG" "$SVG" -resize "${size}x${size}" "$ICON_DIR/$file"
done

# favicon
echo "  favicon.png 32x32 -> $STATIC_DIR/favicon.png"
"$IMG" "$SVG" -resize 32x32 "$STATIC_DIR/favicon.png"

# --- Windows icon (.ico, PNG-compressed entries) --------------------------
echo "  icon.ico (16..256) -> $ICON_DIR/icon.ico"
"$IMG" "$ICON_DIR/icon.png" -define icon:auto-resize=256,128,64,48,32,16 "$ICON_DIR/icon.ico"

# --- macOS icon (.icns, PNG-embedded elements) -----------------------------
# Modern macOS only reads PNG-embedded icon kinds; the legacy RLE RGB/mask
# kinds (is32/s8mk/...) are deliberately not regenerated.
echo "  icon.icns -> $ICON_DIR/icon.icns"
TMP_ICNS="$(mktemp -d)"
trap 'rm -rf "$TMP_ICNS"' EXIT
"$IMG" "$SVG" -resize 32x32 "$TMP_ICNS/ic11.png"
"$IMG" "$SVG" -resize 64x64 "$TMP_ICNS/ic12.png"
"$IMG" "$SVG" -resize 128x128 "$TMP_ICNS/ic07.png"
"$IMG" "$SVG" -resize 256x256 "$TMP_ICNS/ic08.png"
"$IMG" "$SVG" -resize 512x512 "$TMP_ICNS/ic09.png"
"$IMG" "$SVG" -resize 512x512 "$TMP_ICNS/ic13.png"
"$IMG" "$SVG" -resize 1024x1024 "$TMP_ICNS/ic10.png"
"$IMG" "$SVG" -resize 1024x1024 "$TMP_ICNS/ic14.png"
ICON_DIR="$ICON_DIR" TMP_ICNS="$TMP_ICNS" python3 - <<'EOF'
import os, struct
icon_dir = os.environ["ICON_DIR"]
tmp = os.environ["TMP_ICNS"]
# (kind, size): ic11=32, ic12=64, ic07=128, ic08=256, ic09=512,
# ic13=512(retina 256), ic10=1024, ic14=1024(retina 512)
kinds = ["ic11", "ic12", "ic07", "ic08", "ic09", "ic13", "ic10", "ic14"]
body = b""
for kind in kinds:
    with open(os.path.join(tmp, f"{kind}.png"), "rb") as f:
        png = f.read()
    assert png[:8] == b"\x89PNG\r\n\x1a\n", kind
    body += kind.encode("ascii") + struct.pack(">I", 8 + len(png)) + png
with open(os.path.join(icon_dir, "icon.icns"), "wb") as f:
    f.write(b"icns" + struct.pack(">I", 8 + len(body)) + body)
print(f"  packed {len(kinds)} PNG kinds")
EOF

# --- Android adaptive mipmaps -----------------------------------------------
# ic_launcher(.round).png: 48/72/96/144/192; foreground: 108/162/216/324/432.
# XML descriptors (mipmap-anydpi-v26, values) are not images: left alone.
echo "  android mipmaps"
declare -A DENS=(
  ["mdpi"]=48 ["hdpi"]=72 ["xhdpi"]=96 ["xxhdpi"]=144 ["xxxhdpi"]=192
)
for density in mdpi hdpi xhdpi xxhdpi xxxhdpi; do
  size=${DENS[$density]}
  fg=$((size * 108 / 48))
  dir="$ICON_DIR/android/mipmap-$density"
  "$IMG" "$SVG" -resize "${size}x${size}" "$dir/ic_launcher.png"
  # round launcher: same art, inscribed-circle alpha mask
  half=$((size / 2))
  "$IMG" "$SVG" -resize "${size}x${size}" \
    \( +clone -alpha transparent -fill white -draw "circle $half,$half $half,0" \) \
    -compose copyopacity -composite "$dir/ic_launcher_round.png"
  "$IMG" "$SVG" -resize "${fg}x${fg}" "$dir/ic_launcher_foreground.png"
  echo "  mipmap-$density: launcher ${size}, foreground ${fg}"
done

# --- iOS AppIcon set ----------------------------------------------------------
# Pixel size is encoded in the filename: <WxH>@<scale>x, optional -N
# duplicates (83.5x83.5@2x -> 167, 512@2x -> 1024).
echo "  ios AppIcon set"
for src in "$ICON_DIR"/ios/AppIcon-*.png; do
  base="$(basename "$src")"
  size="$(python3 -c "
import re, sys
m = re.match(r'AppIcon-([0-9.]+)(?:x[0-9.]+)?@([0-9]*)x(?:-\d+)?\.png$', '$base')
assert m, 'unparseable: $base'
w, scale = float(m.group(1)), float(m.group(2) or 1)
print(int(w * scale))
")"
  echo "  $base -> ${size}x${size}"
  "$IMG" "$SVG" -resize "${size}x${size}" "$src"
done

echo "Done."
