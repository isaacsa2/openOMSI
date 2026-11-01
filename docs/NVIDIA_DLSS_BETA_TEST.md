# NVIDIA DLSS / DLAA — private beta-testing instructions

**Experimental, not reviewed or merged.** Built from the dedicated fork PR #371.
The Windows binary includes open-source integration code, not NVIDIA DLLs or model weights.
No implication of affiliation with or endorsement from NVIDIA.

## Requirements

- Windows x64, GeForce RTX hardware supported by your NVIDIA driver, and **DirectX 12** as openOMSI's selected backend.
- NVIDIA Streamline **2.14.1**, obtained independently from official NVIDIA sources under its license. Use its matching `sl.interposer.dll`, `sl.common.dll`, `sl.dlss.dll`, and `nvngx_dlss.dll` in a local SDK runtime directory. Do not extract them from the privately distributed OmniLine build.
- Existing, legally installed OMSI 2/openOMSI game content. The beta artifact is the game executable only, not a complete OMSI installation.
- The actual Windows game binary must be compiled with `omsi-render/nvidia-dlss-experimental`.

## Run

In PowerShell, in your openOMSI installation directory (adjust paths):

```powershell
$env:OMSI_DLSS_BETA = "quality"
$env:OMSI_STREAMLINE_DIR = "C:\Path\To\NVIDIA\Streamline\bin\x64"
.\openomsi.exe
```

Replace `quality` with `dlaa`, `balanced`, `performance`, or `ultra_performance`.
Set `OMSI_DLSS_BETA=off` or unset it to disable the beta.
The `dlss=` value in settings.cfg is **not yet connected to this experimental path**.
You do **not** need `OMSI_DLSS_MOTION_DEBUG` when `OMSI_DLSS_BETA` is set.

Note: The beta is for one primary desktop view. VR, mirrors, and triple-screen layouts keep their existing paths. FSR1, FXAA and render scale are suppressed for the beta's 3D view. For a meaningful DLSS benchmark, also select MSAA 1× in the game's graphics settings.

## Test and report

1. Start with DLSS off on exactly the same route, bus, time/weather and window resolution; record FPS and GPU frametime.
2. Try DLAA, Quality, Balanced and Performance; repeat for camera pans, traffic/pedestrians, tree movement and mirrors.
3. Look for ghosting on buses, signs, windshield wipers, transparent glass, rain, sky and HUD.
4. Switch resolution and teleport; temporal history must reset. Check for flicker and frame corruption.
5. Disconnect/omit one official SDK DLL: the log should explain the missing runtime and fall back to the ordinary renderer (no crash).
6. Include GPU, driver, graphics API, DLSS mode, screenshot/video and log with each report.

**This is a candidate for a hardware beta; a successful CI compilation is not proof of valid image quality, runtime compatibility or approval to redistribute NVIDIA binaries.** Keep this branch as Draft until native RTX testing and licensing review are completed.
