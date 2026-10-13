#!/usr/bin/env pwsh
# Windows：便携 zip；有 WiX 3 时再打 MSI。
# 用法：pwsh packaging/windows/build-msi.ps1 -Target <triple>

param(
    [Parameter(Mandatory = $true)][string]$Target
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Resolve-Path (Join-Path $ScriptDir "../..")
Set-Location $RepoRoot

function Get-AppVersion {
    if ($env:APP_VERSION) { return $env:APP_VERSION.TrimStart("v") }
    if ($env:GITHUB_REF_NAME -and $env:GITHUB_REF_NAME.StartsWith("v")) {
        return $env:GITHUB_REF_NAME.TrimStart("v")
    }
    $m = Select-String -Path "Cargo.toml" -Pattern '^version = "([^"]+)"' | Select-Object -First 1
    if (-not $m) { throw "无法从 Cargo.toml 读取 version" }
    return $m.Matches[0].Groups[1].Value
}

function Resolve-BinDir([string]$TargetName) {
    $dir = "target/$TargetName/release"
    if (Test-Path "$dir/transel.exe") { return $dir }
    $host = (rustc -vV | Select-String "^host:").ToString().Split(" ", 2)[1]
    if ($TargetName -eq $host -and (Test-Path "target/release/transel.exe")) {
        return "target/release"
    }
    throw "找不到 $dir/transel.exe，请先 cargo build --release --target $TargetName"
}

function Get-ArchLabel([string]$TargetName) {
    if ($TargetName -match "aarch64|arm64") { return "arm64" }
    return "x86_64"
}

$BinDir = Resolve-BinDir $Target
$Version = Get-AppVersion
$Arch = Get-ArchLabel $Target
$DistDir = Join-Path $RepoRoot "dist"
New-Item -ItemType Directory -Force -Path $DistDir | Out-Null

$Staging = Join-Path $DistDir "staging"
if (Test-Path $Staging) { Remove-Item $Staging -Recurse -Force }
New-Item -ItemType Directory -Force -Path "$Staging/bin" | Out-Null

Copy-Item "$BinDir/transel.exe" "$Staging/bin/transel.exe"
Copy-Item "$BinDir/transel-cli.exe" "$Staging/bin/transel-cli.exe"

$IconFile = Join-Path $ScriptDir "icon.ico"
if (-not (Test-Path $IconFile)) { throw "缺少 packaging/windows/icon.ico" }

$ZipPath = Join-Path $DistDir "Transel-$Version-$Arch.zip"
if (Test-Path $ZipPath) { Remove-Item $ZipPath -Force }
Write-Host "==> 打包 $ZipPath"
Compress-Archive -Path "$Staging/bin/*" -DestinationPath $ZipPath

function Ensure-Wix {
    $candle = Get-Command candle.exe -ErrorAction SilentlyContinue
    if ($candle) { return }
    $local = Join-Path $DistDir "wix/candle.exe"
    if (Test-Path $local) {
        $env:Path = "$(Join-Path $DistDir 'wix');$env:Path"
        return
    }
    Write-Host "==> 下载 WiX 3.14 二进制"
    $zip = Join-Path $DistDir "wix314-binaries.zip"
    $url = "https://github.com/wixtoolset/wix3/releases/download/wix3141rtm/wix314-binaries.zip"
    Invoke-WebRequest -Uri $url -OutFile $zip
    $wixDir = Join-Path $DistDir "wix"
    if (Test-Path $wixDir) { Remove-Item $wixDir -Recurse -Force }
    Expand-Archive -Path $zip -DestinationPath $wixDir
    $env:Path = "$wixDir;$env:Path"
}

try {
    Ensure-Wix
    if (-not (Get-Command candle.exe -ErrorAction SilentlyContinue)) {
        throw "candle.exe 仍不在 PATH"
    }
    Write-Host "==> 编译 .wxs"
    & candle.exe -nologo -ext WixUtilExtension `
        "-dSourceDir=$Staging" `
        "-dVersion=$Version" `
        "-dIconFile=$IconFile" `
        -out "$DistDir\" `
        "$ScriptDir\Product.wxs"
    if ($LASTEXITCODE -ne 0) { throw "candle 失败" }

    $Msi = Join-Path $DistDir "Transel-$Version-$Arch.msi"
    Write-Host "==> 链接 $Msi"
    & light.exe -nologo -ext WixUtilExtension `
        -out $Msi `
        "$DistDir\Product.wixobj"
    if ($LASTEXITCODE -ne 0) { throw "light 失败" }
    Write-Host "==> 完成：$Msi"
}
catch {
    Write-Warning "MSI 未生成：$($_.Exception.Message)"
    Write-Host "==> 已有便携包：$ZipPath"
}

Write-Host "==> Windows 打包完成"
Get-ChildItem $DistDir -File | ForEach-Object { $_.Name }
if (-not (Test-Path $ZipPath)) { exit 1 }
if (-not (Test-Path (Join-Path $DistDir "Transel-$Version-$Arch.msi"))) { exit 1 }
exit 0
