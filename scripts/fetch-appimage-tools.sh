#!/bin/bash
# Download appimagetool to tools/ directory.
# Run once, or re-run to update to latest version.
set -euo pipefail

TOOLS_DIR="$(cd "$(dirname "$0")/../tools" && pwd)"
mkdir -p "$TOOLS_DIR"

echo "Downloading appimagetool to $TOOLS_DIR ..."

curl -fSL -o "$TOOLS_DIR/appimagetool-x86_64.AppImage" \
    "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage"
chmod +x "$TOOLS_DIR/appimagetool-x86_64.AppImage"

echo "Done."
ls -lh "$TOOLS_DIR/appimagetool-x86_64.AppImage"
