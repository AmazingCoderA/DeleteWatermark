#!/usr/bin/env bash
# Сборка APK из Rust cdylib + WebView-оболочки.
# Требует Android SDK (build-tools, platform) и JDK. На CI всё уже есть.
#   SKIP_NDK=1 bash android/build_apk.sh   — пропустить сборку .so (для проверки obolochki)
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"

SDK="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-}}"
if [ -z "$SDK" ]; then
    echo "[ERROR] Установите ANDROID_SDK_ROOT или ANDROID_HOME"
    exit 1
fi
BT=$(ls -d "$SDK"/build-tools/* | sort -V | tail -1)
PLATFORM=$(ls -d "$SDK"/platforms/android-* | sort -V | tail -1)
echo "[INFO] build-tools: $BT"
echo "[INFO] platform:    $PLATFORM"

# На Windows build-tools содержат .bat-файлы, на Linux — обычные бинарники.
_arg() {
    case "$1" in
        /*) command -v cygpath >/dev/null && cygpath -w "$1" || printf '%s' "$1" ;;
        *)  printf '%s' "$1" ;;
    esac
}
run_bt() {
    local name="$1"; shift
    local p="$BT/$name"
    if [ -f "$p.bat" ] && [ ! -f "$p" ]; then
        local args=()
        for a in "$@"; do args+=("$(_arg "$a")"); done
        cmd.exe //c "$(cygpath -w "$p.bat")" "${args[@]}"
    else
        "$p" "$@"
    fi
}

WORK="$ROOT/build_apk"
rm -rf "$WORK"
mkdir -p "$WORK/classes" "$WORK/gen" "$WORK/out/lib/arm64-v8a" "$WORK/out/assets"

# 1. Нативная библиотека с движком.
if [ "${SKIP_NDK:-0}" != "1" ]; then
    echo "[1/6] Building Rust library for arm64-v8a..."
    cargo ndk -t arm64-v8a build --release --no-default-features
else
    echo "[1/6] SKIP_NDK: using existing libdelete_watermark_android.so"
fi
if [ ! -f target/aarch64-linux-android/release/libdelete_watermark_android.so ]; then
    echo "[ERROR] .so not found"
    exit 1
fi
cp target/aarch64-linux-android/release/libdelete_watermark_android.so "$WORK/out/lib/arm64-v8a/"
cp -R assets/GDZ_SAMPLES "$WORK/out/assets/"

# 2. Java-оболочка.
echo "[2/6] Compiling Java..."
javac -source 8 -target 8 -encoding UTF-8 \
    -classpath "$PLATFORM/android.jar" \
    -d "$WORK/classes" \
    android/src/com/amazingcoder/deletewatermark/MainActivity.java

# 3. Dex.
echo "[3/6] D8..."
run_bt d8 --lib "$PLATFORM/android.jar" --min-api 24 \
    --output "$WORK/out" \
    "$WORK/classes/com/amazingcoder/deletewatermark/MainActivity.class"

# 4. Манифест и ресурсы.
echo "[4/6] aapt2..."
run_bt aapt2 compile --dir android/res -o "$WORK/res.flata"
run_bt aapt2 link \
    -o "$WORK/base.apk" \
    -I "$PLATFORM/android.jar" \
    --manifest android/AndroidManifest.xml \
    -R "$WORK/res.flata" \
    --java "$WORK/gen" \
    --min-sdk-version 24 \
    --target-sdk-version 36 \
    --version-code 1 \
    --version-name 8.0 \
    --auto-add-overlay

# 5. Добавляем dex, .so и базу образцов.
echo "[5/6] Packing..."
jar uf "$WORK/base.apk" \
    -C "$WORK/out" classes.dex \
    -C "$WORK/out" lib \
    -C "$WORK/out" assets
run_bt zipalign -p -f 4 "$WORK/base.apk" "$WORK/aligned.apk"

# 6. Подпись.
echo "[6/6] Signing..."
if [ ! -f "$WORK/debug.keystore" ]; then
    keytool -genkeypair \
        -keystore "$WORK/debug.keystore" \
        -storepass android -keypass android \
        -alias androiddebugkey \
        -dname "CN=Android Debug,O=Android,C=US" \
        -validity 10000 -keyalg RSA -keysize 2048 >/dev/null 2>&1
fi
run_bt apksigner sign \
    --ks "$WORK/debug.keystore" \
    --ks-pass pass:android \
    --key-pass pass:android \
    --out "$WORK/delete-watermark.apk" \
    "$WORK/aligned.apk"
run_bt apksigner verify --print-certs "$WORK/delete-watermark.apk"

echo ""
echo "[OK] APK: $WORK/delete-watermark.apk"
