#!/usr/bin/env bash
# Package 超能NONO as a macOS .app bundle and a drag-to-Applications .dmg.
#
#   tools/package_macos.sh              # build (if needed) and package
#   tools/package_macos.sh --skip-build # reuse target/release/nono
#
# Output: dist/超能NONO.app and dist/超能NONO-<version>-m1.dmg
# The bundle is ad-hoc signed.  It is not notarized, so first launch asks the
# user to allow it (see tools/macos/安装说明.txt).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

APP_NAME="超能NONO"
DIST="dist"
WORK="dist-build/package"
ICON="tools/macos/AppIcon.icns"
NOTES="tools/macos/安装说明.txt"
SKIP_BUILD=0

for arg in "$@"; do
  case "$arg" in
    --skip-build) SKIP_BUILD=1 ;;
    -h|--help) sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

VERSION=$(awk -F'"' '/^version = / { print $2; exit }' Cargo.toml)
BUILD=$(date +%Y%m%d)
APP="$DIST/$APP_NAME.app"
DMG="$DIST/$APP_NAME-$VERSION-m1.dmg"

if [[ ! -f "$ICON" ]]; then
  echo "missing $ICON；run tools/make_app_icon.py first (see its --help)" >&2
  exit 1
fi

if [[ "$SKIP_BUILD" -eq 0 ]]; then
  cargo build --release -p nono-app
fi

BIN="target/release/nono"
[[ -x "$BIN" ]] || { echo "missing $BIN" >&2; exit 1; }
if [[ "$(lipo -archs "$BIN" 2>/dev/null)" != *arm64* ]]; then
  echo "$BIN is not an arm64 build; this package targets Apple Silicon" >&2
  exit 1
fi

echo "==> assembling $APP"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
install -m 755 "$BIN" "$APP/Contents/MacOS/nono"
ditto assets "$APP/Contents/Resources/assets"
cp "$ICON" "$APP/Contents/Resources/AppIcon.icns"
sed -e "s/__VERSION__/$VERSION/g" -e "s/__BUILD__/$BUILD/g" \
  tools/macos/Info.plist > "$APP/Contents/Info.plist"
printf 'APPL????' > "$APP/Contents/PkgInfo"
find "$APP" -name .DS_Store -delete
plutil -lint "$APP/Contents/Info.plist" >/dev/null

echo "==> ad-hoc signing"
codesign --force --sign - "$APP"
codesign --verify --strict "$APP"
echo "    $(codesign -dv "$APP" 2>&1 | awk -F= '/Identifier|Signature/ { printf "%s=%s ", $1, $2 }')"

echo "==> staging DMG"
rm -rf "$WORK"
mkdir -p "$WORK/dmg" "$DIST"
ditto "$APP" "$WORK/dmg/$APP_NAME.app"
ln -s /Applications "$WORK/dmg/Applications"
sed "s/__VERSION__/$VERSION/g" "$NOTES" > "$WORK/dmg/安装说明.txt"
chmod 644 "$WORK/dmg/安装说明.txt"
cp "$ICON" "$WORK/dmg/.VolumeIcon.icns"

RW="$WORK/$APP_NAME-rw.dmg"
MNT="$WORK/mnt"
mkdir -p "$MNT"
hdiutil create -quiet -srcfolder "$WORK/dmg" -volname "$APP_NAME" -fs HFS+ \
  -format UDRW -ov "$RW"
hdiutil attach -quiet -nobrowse -noautoopen -mountpoint "$MNT" "$RW"
SetFile -a C "$MNT"
hdiutil detach -quiet "$MNT"
hdiutil convert -quiet "$RW" -format UDZO -imagekey zlib-level=9 -ov -o "$DMG"
rm -rf "$WORK"

cp "$NOTES" "$DIST/安装说明.txt"
sed -i '' "s/__VERSION__/$VERSION/g" "$DIST/安装说明.txt"
chmod 644 "$DIST/安装说明.txt"
(
  cd "$DIST"
  shasum -a 256 "$APP_NAME-$VERSION-m1.dmg" > "SHA256SUMS.txt"
)

echo
echo "built:"
du -sh "$APP" "$DMG"
echo "    $(shasum -a 256 "$DMG")"
