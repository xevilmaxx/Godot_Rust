@echo off
REM launch_engine.bat — builds (first run) and launches the godot_rs engine.
REM Lives inside godot_rs\ so the engine folder is self-contained.
REM Usage:
REM   launch_engine.bat              opens the Godot-style GUI editor
REM   launch_engine.bat launch 240   runs the engine headless for 240 frames
REM   launch_engine.bat help         lists all editor commands
REM   launch_engine.bat mcp          serves the MCP endpoint over stdio
setlocal
cd /d "%~dp0"
where cargo >nul 2>nul
if errorlevel 1 (
  echo [launch_engine] Rust toolchain not found. Install it from https://rustup.rs/ and retry.
  exit /b 1
)
if "%~1"=="" (
  cargo run -q -p editor --features gui -- gui
) else (
  if "%~1"=="gui" (
    cargo run -q -p editor --features gui -- gui
  ) else (
    cargo run -q -p editor -- %*
  )
)
endlocal
