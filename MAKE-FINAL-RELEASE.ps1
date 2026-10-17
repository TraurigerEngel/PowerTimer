param([switch]$SkipBuild)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath $PSScriptRoot
# Windows PowerShell 5.1 does not automatically load the enum assembly.
Add-Type -AssemblyName System.IO.Compression -ErrorAction Stop
Add-Type -AssemblyName System.IO.Compression.FileSystem -ErrorAction Stop

function FileHash([string]$Path) { return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Copy-Required([string]$From,[string]$To) {
  if (-not (Test-Path -LiteralPath $From)) { throw "Required release source is missing: $From" }
  Copy-Item -LiteralPath $From -Destination $To -Recurse -Force
}
# Explicit POSIX separators are important: Windows PowerShell 5.1 uses
# .NET Framework, whose CreateFromDirectory can store backslashes in ZIP names.
# Other ZIP readers (and our verifier) expect the portable forward-slash form.
function New-PortableZip([string]$Folder,[string]$Zip) {
  $root=[System.IO.Path]::GetFullPath($Folder).TrimEnd([char[]]@('\','/'))
  $files=@(Get-ChildItem -LiteralPath $root -File -Recurse | Sort-Object FullName)
  if ($files.Count -eq 0) { throw "Source folder is empty: $Folder" }
  $archive=[System.IO.Compression.ZipFile]::Open($Zip,[System.IO.Compression.ZipArchiveMode]::Create)
  try {
    foreach ($file in $files) {
      $name=$file.FullName.Substring($root.Length).TrimStart([char[]]@('\','/')).Replace('\','/')
      if ([string]::IsNullOrWhiteSpace($name) -or $name.Contains('..')) { throw "Unsafe archive entry: $name" }
      $entry=$archive.CreateEntry($name,[System.IO.Compression.CompressionLevel]::Optimal)
      $inputStream=[System.IO.File]::OpenRead($file.FullName)
      try {
        $outputStream=$entry.Open()
        try { $inputStream.CopyTo($outputStream) } finally { $outputStream.Dispose() }
      } finally { $inputStream.Dispose() }
    }
  } finally { $archive.Dispose() }
}

function Verify-Zip([string]$Zip,[string]$Kind) {
  $z=[System.IO.Compression.ZipFile]::OpenRead($Zip)
  try {
    if ($z.Entries.Count -lt 1) { throw "Empty ZIP: $Zip" }
    $names=@($z.Entries | ForEach-Object { $_.FullName.Replace('\','/') })
    if ($Kind -eq 'portable') {
      if ($names.Count -ne 1 -or $names[0] -ne 'PowerTimer.exe') { throw 'Portable ZIP must contain PowerTimer.exe only.' }
    }
    if ($Kind -eq 'source') {
      foreach ($needed in @('ui/index.html','ui/style.css','src-tauri/Cargo.toml','package.json','MAKE-FINAL-RELEASE.ps1')) {
        if (-not ($names -contains $needed)) { throw "Source ZIP is missing $needed" }
      }
      if ($names | Where-Object { $_ -match '(^|/)(node_modules|target|\.git|Logs|Release|Artifacts)/' }) { throw 'Source ZIP includes build/user artifacts.' }
    }
  } finally { $z.Dispose() }
}

$releaseName='PowerTimer_0.8.0_Stable'
$releaseDir=Join-Path $PSScriptRoot ('Release\'+$releaseName)
$artifactDir=Join-Path $PSScriptRoot 'Artifacts'
$stageDir=Join-Path $PSScriptRoot ('.pt-release-stage-'+[guid]::NewGuid().ToString('N'))
$sourceStage=Join-Path $stageDir 'Source'
$portableStage=Join-Path $stageDir 'Portable'
$artifactStage=Join-Path $stageDir 'Artifacts'
$exe=Join-Path $PSScriptRoot 'src-tauri\target\release\powertimer.exe'

if ((Test-Path -LiteralPath $releaseDir) -or (Test-Path -LiteralPath $artifactDir)) {
  throw 'Release/Artifacts already exist; refusing to overwrite. Use a fresh source folder for another build.'
}
Write-Host '=== PowerTimer 0.8.0 Stable - final release ===' -ForegroundColor Magenta
if ($SkipBuild) {
  Write-Host 'Packaging previously compiled EXE (no rebuild)...' -ForegroundColor Cyan
  $cfg=Get-Content -LiteralPath (Join-Path $PSScriptRoot 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json
  if ($cfg.version -ne '0.8.0' -or $cfg.productName -ne 'PowerTimer') {
    throw 'Unexpected Tauri version or product name; refusing to package.'
  }
  if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw "Release EXE does not exist: $exe" }
  $exeInfo=Get-Item -LiteralPath $exe
  if ($exeInfo.Length -lt 200000) { throw 'Release EXE seems too small.' }
  $fs=[System.IO.File]::OpenRead($exe)
  try {
    $br=New-Object System.IO.BinaryReader($fs)
    try {
      if ($br.ReadUInt16() -ne 0x5A4D) { throw 'EXE MZ header missing.' }
      $fs.Position=0x3C; $peOffset=$br.ReadInt32()
      if ($peOffset -lt 64 -or $peOffset -gt ($fs.Length-100)) { throw 'EXE has invalid PE offset.' }
      $fs.Position=$peOffset
      if ($br.ReadUInt32() -ne 0x00004550) { throw 'EXE PE header missing.' }
      $fs.Position=$peOffset+24+68
      if ($br.ReadUInt16() -ne 2) { throw 'EXE does not use Windows GUI subsystem.' }
    } finally { $br.Dispose() }
  } finally { $fs.Dispose() }
  Write-Host 'Previously compiled Windows GUI EXE verified.' -ForegroundColor Green
} else {
  & (Join-Path $PSScriptRoot 'BUILD-RELEASE.ps1')
  # BUILD-RELEASE.ps1 throws on failures. Never publish failed builds.
}
try {
  foreach ($dir in @($portableStage,$sourceStage,$artifactStage)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
  Copy-Required $exe (Join-Path $portableStage 'PowerTimer.exe')

  # Whitelist sources. Never pack local timer data, node_modules, rust target,
  # user logs, secret settings, .bak files, or the existing Windows executable.
  foreach ($name in @('ui','tests','legacy')) {
    Copy-Required (Join-Path $PSScriptRoot $name) (Join-Path $sourceStage $name)
  }
  $srcTauri=Join-Path $sourceStage 'src-tauri'
  New-Item -ItemType Directory -Path $srcTauri -Force | Out-Null
  foreach ($name in @('src','icons','capabilities')) {
    Copy-Required (Join-Path $PSScriptRoot ('src-tauri\'+$name)) (Join-Path $srcTauri $name)
  }
  foreach ($name in @('Cargo.toml','build.rs','tauri.conf.json','Cargo.lock')) {
    $from=Join-Path $PSScriptRoot ('src-tauri\'+$name)
    if (Test-Path -LiteralPath $from) { Copy-Item -LiteralPath $from -Destination (Join-Path $srcTauri $name) -Force }
  }
  foreach ($name in @('.gitignore','package.json','package-lock.json','Build-Windows.cmd',
      'BUILD-RELEASE.ps1','MAKE-FINAL-RELEASE.ps1','README_RU.md','RELEASE-CHECKLIST.md',
      'RELEASE_NOTES_0.8.0.md','LICENSE.txt','AUDIT_RU.md','QA_WINDOWS_RU.md','RC2_PROGRESS_RU.md')) {
    $from=Join-Path $PSScriptRoot $name
    if (Test-Path -LiteralPath $from) { Copy-Item -LiteralPath $from -Destination (Join-Path $sourceStage $name) -Force }
  }

  $portableZip=Join-Path $artifactStage ($releaseName+'_Portable.zip')
  $sourceZip=Join-Path $artifactStage ($releaseName+'_Source.zip')
  New-PortableZip $portableStage $portableZip
  New-PortableZip $sourceStage $sourceZip
  Verify-Zip $portableZip 'portable'
  Verify-Zip $sourceZip 'source'

  $hashExe=FileHash (Join-Path $portableStage 'PowerTimer.exe')
  $hashZipPortable=FileHash $portableZip
  $hashZipSource=FileHash $sourceZip
  @(
    "$hashExe  PowerTimer.exe",
    "$hashZipPortable  $([IO.Path]::GetFileName($portableZip))",
    "$hashZipSource  $([IO.Path]::GetFileName($sourceZip))"
  ) | Set-Content -LiteralPath (Join-Path $artifactStage 'SHA256SUMS.txt') -Encoding ASCII
  [ordered]@{ product='PowerTimer';version='0.8.0';channel='Stable';buildDate=(Get-Date).ToString('o');portableExe='PowerTimer.exe';portableSha256=$hashExe;portableZipSha256=$hashZipPortable;sourceZipSha256=$hashZipSource;webView2RuntimeRequired=$true } |
    ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $artifactStage 'RELEASE-METADATA.json') -Encoding UTF8

  $releaseRoot=Join-Path $PSScriptRoot 'Release'
  New-Item -ItemType Directory -Path $releaseRoot -Force | Out-Null
  if ((Test-Path -LiteralPath $releaseDir) -or (Test-Path -LiteralPath $artifactDir)) { throw 'Release destination became occupied during build.' }
  Move-Item -LiteralPath $portableStage -Destination $releaseDir
  Move-Item -LiteralPath $artifactStage -Destination $artifactDir
  Write-Host ''
  Write-Host '=== FILES ARE READY ===' -ForegroundColor Green
  Write-Host "Standalone EXE: $(Join-Path $releaseDir 'PowerTimer.exe')"
  Write-Host "Portable ZIP:   $(Join-Path $artifactDir ($releaseName+'_Portable.zip'))"
  Write-Host "Source ZIP:     $(Join-Path $artifactDir ($releaseName+'_Source.zip'))"
  Write-Host "SHA256:         $(Join-Path $artifactDir 'SHA256SUMS.txt')"
  Write-Host 'Before publishing, run the portable EXE separately and test timer/tray/exit in test mode.' -ForegroundColor Yellow
} finally {
  if (Test-Path -LiteralPath $stageDir) { Remove-Item -LiteralPath $stageDir -Recurse -Force }
}
