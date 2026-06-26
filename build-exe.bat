@echo off
chcp 65001 >nul
echo Building Vantage Draft Assistant to .exe ...
echo.
cd /d "%~dp0"

where cargo >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Rust/Cargo not found in PATH.
    echo Install Rust from https://rustup.rs and restart the terminal.
    pause
    exit /b 1
)

call npm run tauri build
if errorlevel 1 (
    echo.
    echo Build failed. Check errors above.
    pause
    exit /b 1
)

echo.
echo Done. .exe and installer are in: src-tauri\target\release\
echo Installer (NSIS): src-tauri\target\release\bundle\nsis\
pause
