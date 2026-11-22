#!/data/data/com.termux/files/usr/bin/bash
# Сборка прямо на Android-устройстве в Termux (без NDK на ПК).
#   pkg install rust binutils git
#   bash build_termux.sh
set -euo pipefail
cd "$(dirname "$0")"

echo "[1/2] cargo build --release --no-default-features"
cargo build --release --no-default-features

echo "[2/2] Installing to \$PREFIX/bin"
cp -f target/release/delete-watermark "$PREFIX/bin/delete-watermark"
chmod +x "$PREFIX/bin/delete-watermark"

mkdir -p "$HOME/.local/share/delete-watermark"
cp -R assets/. "$HOME/.local/share/delete-watermark/"

cat > "$PREFIX/bin/delete-watermark-web" <<'EOF'
#!/data/data/com.termux/files/usr/bin/bash
export WATERMARK_SAMPLES="$HOME/.local/share/delete-watermark/GDZ_SAMPLES"
exec delete-watermark --mode web --port 5000
EOF
chmod +x "$PREFIX/bin/delete-watermark-web"

echo "[OK] Готово."
echo "     CLI:  delete-watermark --mode cli --input photo.jpg"
echo "     Web:  delete-watermark-web  (затем откройте http://127.0.0.1:5000)"
