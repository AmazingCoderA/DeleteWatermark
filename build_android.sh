#!/usr/bin/env bash
# Кроссплатформенная сборка под Android (CLI + Web, без GUI).
# Требуется Android NDK и cargo-ndk:
#   cargo install cargo-ndk
#   export ANDROID_NDK_HOME=/path/to/android-ndk
#   ./build_android.sh [arm64-v8a] [armeabi-v7a]
set -euo pipefail
cd "$(dirname "$0")"

TARGETS=("${@:-arm64-v8a}")
command -v cargo >/dev/null || { echo "[ERROR] cargo not found"; exit 1; }
command -v cargo-ndk >/dev/null || { echo "[ERROR] cargo-ndk not found: cargo install cargo-ndk"; exit 1; }
: "${ANDROID_NDK_HOME:?Установите ANDROID_NDK_HOME}"

mkdir -p dist/android/assets
cp -R assets/. dist/android/assets/

for abi in "${TARGETS[@]}"; do
    echo "[*] Building for $abi ..."
    cargo ndk -t "$abi" -o dist/android/libs \
        build --release --no-default-features
done

# cargo-ndk копирует cdylib в dist/android/libs; бинарники берём из target/.
if [[ -f target/aarch64-linux-android/release/delete-watermark ]]; then
    cp -f target/aarch64-linux-android/release/delete-watermark dist/android/delete-watermark-arm64
fi
if [[ -f target/armv7-linux-androideabi/release/delete-watermark ]]; then
    cp -f target/armv7-linux-androideabi/release/delete-watermark dist/android/delete-watermark-armv7
fi

cat > dist/android/run-web.sh <<'EOF'
#!/system/bin/sh
# Запуск веб-интерфейса прямо на устройстве (Termux / adb shell).
cd "$(dirname "$0")"
export WATERMARK_SAMPLES="$PWD/assets/GDZ_SAMPLES"
./delete-watermark-arm64 --mode web --port 5000
EOF
chmod +x dist/android/run-web.sh

echo ""
echo "[OK] Готово: dist/android/"
echo "     Перенесите папку на устройство: adb push dist/android /sdcard/delete-watermark"
echo "     Затем: cd /sdcard/delete-watermark && sh run-web.sh"
