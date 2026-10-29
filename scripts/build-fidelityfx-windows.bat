@echo off
setlocal EnableExtensions
rem Build a Windows test binary with actual FidelityFX temporal backends.
rem FSR2 is WGSL; FSR3 remains optional, hidden in the normal menu.
rem Usage: scripts\build-fidelityfx-windows.bat [fsr2^|both]
rem No executable is labelled FSR2 if the Rust shaders fail to compile.

set "ROOT=%~dp0.."
if not exist "%ROOT%\Cargo.toml" (
  echo [ERROR] This script must live in the openOMSI scripts directory.
  exit /b 2
)
where cargo.exe >nul 2>nul
if errorlevel 1 (
  echo [ERROR] Rust/Cargo not installed. Install Rust with MSVC tooling.
  exit /b 2
)
set "FX_FEATURES=omsi-render/fsr2-wgsl"
if /I "%~1"=="both" set "FX_FEATURES=omsi-render/fsr2-wgsl,omsi-render/fsr3-wgpu"
if not "%~1"=="" if /I not "%~1"=="fsr2" if /I not "%~1"=="both" (
  echo [ERROR] Usage: %~nx0 [fsr2^|both]
  exit /b 2
)
pushd "%ROOT%" || exit /b 2
echo [INFO] Building experimental FidelityFX features: %FX_FEATURES%
echo [INFO] Shader capability is checked at runtime; this build does not guarantee all GPUs work.
cargo build --locked --release -p omsi-app --features "%FX_FEATURES%"
set "STATUS=%ERRORLEVEL%"
if not "%STATUS%"=="0" (
  echo [ERROR] The experimental FidelityFX backend failed to compile. No test binary was produced.
  popd
  exit /b %STATUS%
)
if not exist "target\release\openomsi.exe" (
  echo [ERROR] Cargo completed but the game executable is missing.
  popd
  exit /b 3
)
echo [SUCCESS] target\release\openomsi.exe
echo [INFO] Select AMD FSR 2.0 in Graphics. Check logs for
echo        FSR2_WGSL_BENCH_DISPATCH_OK, missing GPU features or validation errors.
echo [INFO] No CPU fallback is silently advertised as native FSR 2.
popd
exit /b 0
