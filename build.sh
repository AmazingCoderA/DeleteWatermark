#!/usr/bin/env bash
# Автосборка для Linux / macOS.
#   ./build.sh              — с GUI (нужны libgtk-3-dev / X11)
#   ./build.sh headless     — только CLI + Web (без системных GUI-зависимостей)
set -euo pipefail
cd "$(dirname "$0")"

echo "============================================================"
echo " AUTOBUILD: delete-watermark (Linux/macOS)"
echo "============================================================"

command -v cargo >/dev/null || { echo "[ERROR] cargo not found: https://rustup.rs"; exit 1; }

FEATURES=()
if [[ "${1:-}" == "headless" ]]; then
    FEATURES=(--no-default-features)
    echo "[INFO] GUI отключён (CLI + Web)"
fi

echo "[1/3] Building release binary..."
cargo build --release "${FEATURES[@]}"

echo "[2/3] Preparing dist/..."
mkdir -p dist
cp -f target/release/delete-watermark dist/
rm -rf dist/assets
cp -R assets dist/
cp -f README.md dist/

echo "[3/3] Packing archive..."
ARCHIVE="delete-watermark-v8.0-$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m).tar.gz"
tar -czf "dist/${ARCHIVE}" -C dist delete-watermark assets README.md

echo ""
echo "[OK] Done: dist/${ARCHIVE}"
echo "============================================================"
