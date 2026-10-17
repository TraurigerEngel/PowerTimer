Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath $PSScriptRoot

function Assert-ExitCode([string]$Step) {
  if ($LASTEXITCODE -ne 0) { throw "$Step failed (exit code $LASTEXITCODE)." }
}
function Assert-WindowsGuiExe([string]$ExePath) {
  if (-not (Test-Path -LiteralPath $ExePath -PathType Leaf)) { throw "EXE not found: $ExePath" }
  $file = Get-Item -LiteralPath $ExePath
  if ($file.Length -lt 200000) { throw 'Release EXE seems unusually small.' }
  $fs=[System.IO.File]::OpenRead($ExePath)
  try {
    $br = New-Object System.IO.BinaryReader($fs)
    try {
      if ($br.ReadUInt16() -ne 0x5A4D) { throw 'MZ header missing.' }
      $fs.Position=0x3C; $peOffset=$br.ReadInt32()
      if ($peOffset -lt 64 -or $peOffset -gt ($fs.Length - 100)) { throw 'Invalid PE header offset.' }
      $fs.Position=$peOffset
      if ($br.ReadUInt32() -ne 0x00004550) { throw 'PE header missing.' }
      $fs.Position=$peOffset+24
      $magic=$br.ReadUInt16()
      if ($magic -ne 0x20B -and $magic -ne 0x10B) { throw 'Unsupported Windows PE type.' }
      $fs.Position=$peOffset+24+68
      $subsystem=$br.ReadUInt16()
      if ($subsystem -ne 2) { throw "Expected GUI subsystem 2, found $subsystem." }
    } finally { $br.Dispose() }
  } finally { $fs.Dispose() }
}

$manifest=Join-Path $PSScriptRoot 'src-tauri\Cargo.toml'
$exe=Join-Path $PSScriptRoot 'src-tauri\target\release\powertimer.exe'
$cfg=Get-Content -LiteralPath (Join-Path $PSScriptRoot 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json
if ($cfg.version -ne '0.8.0' -or $cfg.productName -ne 'PowerTimer') { throw 'Unexpected Tauri version or product name.' }
foreach ($name in @('node','npm.cmd','rustc','cargo')) {
  if (-not (Get-Command $name -ErrorAction SilentlyContinue)) { throw "$name is missing." }
}
Write-Host '[1/4] Installing npm dependencies...' -ForegroundColor Cyan
& npm.cmd install --no-audit --no-fund
Assert-ExitCode 'npm install'
Write-Host '[2/4] Checking JavaScript and UI regressions...' -ForegroundColor Cyan
& npm.cmd run check
Assert-ExitCode 'npm check'
Write-Host '[3/4] Running Rust unit tests...' -ForegroundColor Cyan
& cargo test --manifest-path $manifest
Assert-ExitCode 'cargo test'
Write-Host '[4/4] Building release EXE...' -ForegroundColor Cyan
& npm.cmd run build
Assert-ExitCode 'npm build'
Assert-WindowsGuiExe $exe
Write-Host "Release EXE passed basic PE/GUI checks: $exe" -ForegroundColor Green
