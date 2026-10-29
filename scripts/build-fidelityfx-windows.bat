@echo off
setlocal EnableExtensions
rem Build Windows openOMSI with FSR2 WGSL (enabled in the default renderer).
rem Usage: scripts\build-fidelityfx-windows.bat
set "ROOT=%~dp0.."
if not exist "%ROOT%\Cargo.toml" (
 echo [ERROR] Run this script from the openOMSI scripts directory.
 exit /b 2
)
where cargo.exe >nul 2>nul
if errorlevel 1 (
 echo [ERROR] Rust/Cargo not installed.
 exit /b 2
)
if not "%~1"=="" (
 echo [ERROR] No arguments accepted. Only FSR2 WGSL is supported.
 exit /b 2
)
pushd "%ROOT%" || exit /b 2
cargo build --locked --release -p omsi-app
set "RESULT=%ERRORLEVEL%"
if not "%RESULT%"=="0" (
 popd
 echo [ERROR] FSR2 WGSL compilation failed.
 exit /b %RESULT%
)
if not exist "target\release\openomsi.exe" (
 popd
 echo [ERROR] Executable missing.
 exit /b 3
)
echo [OK] target\release\openomsi.exe
echo [INFO] If FSR2 cannot run, check the graphics log for missing device features.
popd
exit /b 0
