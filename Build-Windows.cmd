@echo off
chcp 65001 >nul
cd /d "%~dp0"
echo ===========================================
echo  PowerTimer 0.8.0 Stable — Tauri
echo ===========================================
echo.
where npm >nul 2>&1 || (echo [ERROR] Node.js/npm is missing. & pause & exit /b 1)
where cargo >nul 2>&1 || (echo [ERROR] Rust/Cargo is missing. & pause & exit /b 1)
echo [1/3] Installing frontend build dependency...
call npm install || goto :fail
echo [2/3] Building portable executable...
call npm run build || goto :fail
echo [3/3] Done.
echo.
echo EXE: src-tauri\target\release\powertimer.exe
echo A single exe is enough to launch (with system WebView2 runtime).
pause
exit /b 0
:fail
echo.
echo Build failed. Read the errors above.
pause
exit /b 1
