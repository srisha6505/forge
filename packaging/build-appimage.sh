#!/usr/bin/env bash
#
# Build a portable AppImage for Forge.
#
# Prerequisites:
#   - Rust toolchain (rustup)
#   - System packages: cmake, pkg-config, libvulkan-dev, glslc (or glslang-tools + glslc)
#   - appimagetool (downloaded automatically if missing)
#
# Usage:
#   ./packaging/build-appimage.sh
#
# Output:
#   Forge-x86_64.AppImage in the repo root

set -euo pipefail
cd "$(dirname "$0")/.."

APPDIR="$(pwd)/Forge.AppDir"
ARCH="x86_64"

echo "==> Building release binary..."
cargo build --release

echo "==> Preparing AppDir..."
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin"
mkdir -p "$APPDIR/usr/lib"
mkdir -p "$APPDIR/usr/share/applications"
mkdir -p "$APPDIR/usr/share/icons/hicolor/256x256/apps"

# Binary
cp target/release/forge "$APPDIR/usr/bin/forge"
strip "$APPDIR/usr/bin/forge" 2>/dev/null || true

# Bundled libs (xcb, xkbcommon for X11 support)
if [ -d "lib" ]; then
    cp lib/*.so "$APPDIR/usr/lib/" 2>/dev/null || true
fi

# Desktop file and icon
cp packaging/forge.desktop "$APPDIR/usr/share/applications/"
cp packaging/forge.desktop "$APPDIR/"

# Generate a simple SVG icon if no PNG exists
if [ -f "packaging/forge.png" ]; then
    cp packaging/forge.png "$APPDIR/usr/share/icons/hicolor/256x256/apps/forge.png"
    cp packaging/forge.png "$APPDIR/forge.png"
else
    echo "Warning: No icon found at packaging/forge.png, AppImage will have no icon"
    # Create a minimal 1x1 placeholder
    printf '\x89PNG\r\n\x1a\n' > "$APPDIR/forge.png"
fi

# AppRun script
cat > "$APPDIR/AppRun" << 'APPRUN'
#!/usr/bin/env bash
SELF="$(readlink -f "$0")"
APPDIR="$(dirname "$SELF")"
export LD_LIBRARY_PATH="${APPDIR}/usr/lib:${LD_LIBRARY_PATH:-}"
exec "${APPDIR}/usr/bin/forge" "$@"
APPRUN
chmod +x "$APPDIR/AppRun"

# Download appimagetool if not available
APPIMAGETOOL=""
if command -v appimagetool &>/dev/null; then
    APPIMAGETOOL="appimagetool"
else
    echo "==> Downloading appimagetool..."
    TOOL_PATH="/tmp/appimagetool-x86_64.AppImage"
    if [ ! -f "$TOOL_PATH" ]; then
        curl -Lo "$TOOL_PATH" "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage"
        chmod +x "$TOOL_PATH"
    fi
    APPIMAGETOOL="$TOOL_PATH"
fi

echo "==> Building AppImage..."
ARCH="$ARCH" "$APPIMAGETOOL" "$APPDIR" "Forge-${ARCH}.AppImage"

# Cleanup
rm -rf "$APPDIR"

echo ""
echo "==> Done: Forge-${ARCH}.AppImage"
echo "    Run with: ./Forge-${ARCH}.AppImage"
ls -lh "Forge-${ARCH}.AppImage"
