#!/bin/bash
# Build an AppImage for PackHound.
# The AppImage packages the binary, icon, and desktop file. GTK4 and
# libadwaita are expected on the host system (standard on modern desktops).
# Prerequisites: run scripts/fetch-appimage-tools.sh first.
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
TOOLS_DIR="$PROJECT_DIR/tools"
ASSETS_DIR="$PROJECT_DIR/assets"
BUILD_DIR="/tmp/packhound-appimage-build"

APPIMAGETOOL="$TOOLS_DIR/appimagetool-x86_64.AppImage"

# Extract version from Cargo.toml
VERSION=$(grep '^version' "$PROJECT_DIR/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')
APP_ID="com.github.packhound"
OUTPUT="$PROJECT_DIR/PackHound-${VERSION}-x86_64.AppImage"

echo "=== PackHound AppImage Builder v${VERSION} ==="

if [ ! -x "$APPIMAGETOOL" ]; then
    echo "ERROR: Missing appimagetool: $APPIMAGETOOL"
    echo "Run: scripts/fetch-appimage-tools.sh"
    exit 1
fi

# Build release binary
echo ""
echo "=== Building release binary ==="
cd "$PROJECT_DIR"
cargo build --release

# Strip the binary
echo ""
echo "=== Stripping binary ==="
strip "$PROJECT_DIR/target/release/addon-manager"
ls -lh "$PROJECT_DIR/target/release/addon-manager"

# Build AppDir
echo ""
echo "=== Building AppDir ==="
rm -rf "$BUILD_DIR"
APPDIR="$BUILD_DIR/PackHound.AppDir"
mkdir -p "$APPDIR/usr/bin"
mkdir -p "$APPDIR/usr/share/applications"
mkdir -p "$APPDIR/usr/share/icons/hicolor/256x256/apps"

# Binary
cp "$PROJECT_DIR/target/release/addon-manager" "$APPDIR/usr/bin/"

# Desktop file
cp "$PROJECT_DIR/${APP_ID}.desktop" "$APPDIR/"
cp "$PROJECT_DIR/${APP_ID}.desktop" "$APPDIR/usr/share/applications/"

# Icon
cp "$ASSETS_DIR/${APP_ID}.png" "$APPDIR/${APP_ID}.png"
cp "$ASSETS_DIR/${APP_ID}.png" "$APPDIR/usr/share/icons/hicolor/256x256/apps/${APP_ID}.png"

# AppRun
cat > "$APPDIR/AppRun" << 'EOF'
#!/bin/bash
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/addon-manager" "$@"
EOF
chmod +x "$APPDIR/AppRun"

# Build the AppImage
echo ""
echo "=== Creating AppImage ==="
ARCH=x86_64 "$APPIMAGETOOL" --comp zstd "$APPDIR" "$OUTPUT"
chmod +x "$OUTPUT"

# Clean up
rm -rf "$BUILD_DIR"

echo ""
echo "=== Done ==="
ls -lh "$OUTPUT"
echo "Run with: ./$( basename "$OUTPUT" )"
