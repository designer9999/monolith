@echo off
setlocal

cd /d "%~dp0"

where npm >nul 2>nul
if errorlevel 1 (
  echo npm was not found on PATH.
  echo Install Node.js, then run this file again.
  pause
  exit /b 1
)

REM Install when dependencies are missing OR when the Windows CLI shim is absent
REM (e.g. node_modules was last installed under WSL, which only writes Linux shims).
if not exist "node_modules\.bin\tauri.cmd" (
  echo Installing project dependencies for Windows...
  call npm install
  if errorlevel 1 (
    echo.
    echo Dependency install failed.
    pause
    exit /b 1
  )
)

powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\prepare-gui.ps1" -Root "%CD%"
if errorlevel 1 (
  echo.
  echo Startup preparation failed.
  pause
  exit /b 1
)

echo Launching MONOLITH desktop GUI...
echo Rust watcher is disabled for stable startup. Frontend hot reload still works.
echo Restart this window after backend/Rust/icon changes.
call npm run gui

echo.
echo MONOLITH GUI stopped.
pause
