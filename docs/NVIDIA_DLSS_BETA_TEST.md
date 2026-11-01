# NVIDIA DLSS / DLAA — private beta-testing instructions

**Experimental, not reviewed or merged.** Built from fork PR #371. The Windows x64 package now has **one game executable**, with DLSS/DLAA compiled in but disabled by default, rather than a separate DLSS edition.
The standard Windows x64 archive now packages the **official NVIDIA-signed Streamline 2.14.1 production DLLs**, with the upstream license notices. No separate DLSS edition is produced and DLSS remains off by default.
No implication of affiliation with or endorsement from NVIDIA.

## Requirements

- Windows x64, GeForce RTX hardware supported by your NVIDIA driver, and **DirectX 12** as openOMSI's selected backend.
- DLSS/DLAA controls are visible only on Windows PCs where DXGI identifies an NVIDIA RTX GPU candidate and DX12 is selected. In-game, the selected rendering adapter must also be an RTX GPU running DX12. A PC with Intel/AMD rendering selected will not display DLSS merely because a second RTX card is installed.
- The RTX preflight is conservative and is not a promise of Streamline compatibility: official `slIsFeatureSupported` and successful runtime initialization remain mandatory when you activate DLSS. An unsupported or unidentified GPU uses the original rendering path.
- The normal Windows x64 release ZIP contains `sl.interposer.dll`, `sl.common.dll`, `sl.dlss.dll`, and `nvngx_dlss.dll` from NVIDIA's official **production SDK 2.14.1**. The packaging workflow checks NVIDIA's release ZIP SHA-256 and validates each DLL's NVIDIA Authenticode signature. No third-party OmniLine binaries are used. Manually compiled or older versions can still specify an independent `OMSI_STREAMLINE_DIR`.
- Existing, legally installed OMSI 2/openOMSI game content. The standard Windows package contains the game and companion launchers for use with an existing, legitimate OMSI 2/openOMSI installation; it is not a complete OMSI content installation.
- The **standard Windows x64 build script** (`scripts/build-windows.cmd`) already compiles the optional native DLSS adapter into `openomsi.exe`. DLSS remains disabled by default. Custom manual `cargo build` commands require `--features omsi-render/nvidia-dlss-experimental`.

## Run

In PowerShell, in your openOMSI installation directory (adjust paths):

```powershell
$env:OMSI_DLSS_BETA = "quality"
$env:OMSI_STREAMLINE_DIR = "C:\Path\To\NVIDIA\Streamline\bin\x64"
.\openomsi.exe
```

Replace `quality` with `dlaa`, `balanced`, `performance`, or `ultra_performance`.
Set `OMSI_DLSS_BETA=off` or unset it to disable the beta.
NVIDIA's official integration guide specifies a NVIDIA-issued application ID for NGX components.
If your SDK access provides an authorized ID, set `OMSI_DLSS_APPLICATION_ID` to that numeric ID.
The historical public adapter's hard-coded project UUID has **not** been verified as registered.
A zero/missing application ID may prevent DLSS from initializing; do not assume that a successful build is enough. In the reported 2026-11-01 RTX test, Streamline 2.14.1 initialized with the DLSS plugin. That verifies initialization only, not correctness of the final image.

The launcher and game graphics menus now save `dlss=` for Windows DX12; the mode takes effect after restarting. The environment variable is only an override for diagnostics.
You do **not** need `OMSI_DLSS_MOTION_DEBUG` when `OMSI_DLSS_BETA` is set.

Note: The beta is for one primary desktop view. VR, mirrors, and triple-screen layouts keep their existing paths. FSR1, FXAA and render scale are suppressed for the beta's 3D view. For a meaningful DLSS benchmark, also select MSAA 1× in the game's graphics settings.

## Test and report

1. Start with DLSS off on exactly the same route, bus, time/weather and window resolution; record FPS and GPU frametime.
2. Try DLAA, Quality, Balanced and Performance; repeat for camera pans, traffic/pedestrians, tree movement and mirrors.
3. Look for ghosting on buses, signs, windshield wipers, transparent glass, rain, sky and HUD.
4. Switch resolution and teleport; temporal history must reset. Check for flicker and frame corruption.
5. Disconnect/omit one official SDK DLL: the log should explain the missing runtime and fall back to the ordinary renderer (no crash).
6. Include GPU, driver, graphics API, DLSS mode, screenshot/video and log with each report.

**The 2026-11-01 hardware log confirmed Streamline initialized and computed Balanced render size, then wgpu rejected the sky-motion pass because camera bind group 0 was missing.** The pass binding order is corrected on this branch. Please retest it on the same RTX system, especially Enhanced+ mode. A successful CI build is not proof of valid image quality. Keep the PR in Draft until the GPU fix and license compatibility are reviewed.
