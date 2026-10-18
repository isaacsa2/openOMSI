# Compile the experimental O3D normal shader with the matching BGFX shaderc.
# This never reads or copies original OMSI 2 content.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Shaderc,
    [Parameter(Mandatory = $true)][string]$BgfxSource,
    [ValidateSet('dx11','dx12','vulkan','gl')][string]$Backend = 'dx11'
)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path -LiteralPath $Shaderc -PathType Leaf)) { throw "shaderc not found: $Shaderc" }
$include = Join-Path $BgfxSource 'src'
if (-not (Test-Path -LiteralPath (Join-Path $include 'bgfx_shader.sh'))) {
    throw "BGFX include directory missing src/bgfx_shader.sh: $include"
}
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$shaderRoot = Join-Path $here 'shaders'
$out = Join-Path (Join-Path $here 'compiled') $Backend
New-Item -ItemType Directory -Path $out -Force | Out-Null
$profiles = @{
    dx11 = @{ Platform='windows'; Profile='s_5_0' }
    # DX12/HLSL compatibility requires testing on a real GPU.
    dx12 = @{ Platform='windows'; Profile='s_5_0' }
    vulkan = @{ Platform='windows'; Profile='spirv' }
    gl = @{ Platform='windows'; Profile='430' }
}
$platform = $profiles[$Backend].Platform
$profile = $profiles[$Backend].Profile
foreach ($stage in @(@{Name='vs_mesh'; Type='vertex'}, @{Name='fs_mesh'; Type='fragment'})) {
    $inputFile = Join-Path $shaderRoot ($stage.Name + '.sc')
    $outputFile = Join-Path $out ($stage.Name + '.bin')
    $arguments = @('-f', $inputFile, '-o', $outputFile, '--type', $stage.Type,
        '--platform', $platform, '--profile', $profile,
        '--varyingdef', (Join-Path $shaderRoot 'varying.def.sc'), '-i', $include)
    & $Shaderc @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "shaderc failed compiling $($stage.Name) for $Backend (exit $LASTEXITCODE)"
    }
    if ((Get-Item -LiteralPath $outputFile).Length -eq 0) {
        throw "shaderc produced an empty output: $outputFile"
    }
}
Write-Output "Compiled shader binaries in: $out"
Write-Output 'Normal shading only; this is not full OMSI material rendering.'
