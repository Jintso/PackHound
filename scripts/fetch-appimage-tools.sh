#!/bin/bash
# Download appimagetool to tools/ directory.
# Run once, or re-run to update to latest version.
set -euo pipefail

TOOLS_DIR="$(dirname "$0")/../tools"
mkdir -p "$TOOLS_DIR"
TOOLS_DIR="$(cd "$TOOLS_DIR" && pwd)"

echo "Downloading appimagetool to $TOOLS_DIR ..."

curl -fSL -o "$TOOLS_DIR/appimagetool-x86_64.AppImage" \
    "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage"
chmod +x "$TOOLS_DIR/appimagetool-x86_64.AppImage"

echo "Done."
ls -lh "$TOOLS_DIR/appimagetool-x86_64.AppImage"
