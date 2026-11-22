@echo off
chcp 65001 >nul
setlocal enabledelayedexpansion
cd /d "%~dp0"

echo ============================================================
echo  AUTOBUILD: delete-watermark (Windows)
echo ============================================================

where cargo >nul 2>nul
if errorlevel 1 (
    echo [ERROR] Rust/cargo not found. Install https://rustup.rs
    exit /b 1
)

set "FEATURES="
if "%~1"=="headless" set "FEATURES=--no-default-features"

echo [1/3] Building release binary...
cargo build --release %FEATURES%
if errorlevel 1 (
    echo [ERROR] cargo build failed
    exit /b 1
)

echo [2/3] Preparing dist\...
if not exist dist mkdir dist
copy /Y "target\release\delete-watermark.exe" "dist\" >nul
xcopy /E /I /Y "assets" "dist\assets" >nul
copy /Y "README.md" "dist\" >nul
copy /Y "README_RU.md" "dist\" >nul
xcopy /E /I /Y "docs" "dist\docs" >nul

echo [3/3] Packing ZIP...
set "ZIPNAME=delete-watermark-v8.0-windows-x86_64.zip"
if exist "dist\%ZIPNAME%" del "dist\%ZIPNAME%"
powershell -NoProfile -Command "Compress-Archive -Path 'dist\delete-watermark.exe','dist\assets','dist\README.md','dist\README_RU.md','dist\docs' -DestinationPath 'dist\%ZIPNAME%' -Force"

echo.
echo [OK] Done: dist\%ZIPNAME%
echo ============================================================
endlocal
