#!/usr/bin/env pwsh
# Windows 打包入口：可选编译，再打 zip + MSI。
#
#   pwsh packaging/package.ps1
#   pwsh packaging/package.ps1 -Build
#   pwsh packaging/package.ps1 -Build -Target x86_64-pc-windows-msvc

[CmdletBinding()]
param(
    [string]$Target = "",
    [switch]$Build
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Resolve-Path (Join-Path $ScriptDir "..")
Set-Location $RepoRoot

function Get-HostTriple {
    $line = rustc -vV | Select-String "^host:"
    return ($line.ToString() -split "\s+")[1]
}

function Get-AppVersion {
    if ($env:APP_VERSION) { return $env:APP_VERSION.TrimStart("v") }
    if ($env:GITHUB_REF_NAME -and $env:GITHUB_REF_NAME.StartsWith("v")) {
        return $env:GITHUB_REF_NAME.TrimStart("v")
    }
    $m = Select-String -Path "Cargo.toml" -Pattern '^version = "([^"]+)"' | Select-Object -First 1
    if (-not $m) { throw "无法从 Cargo.toml 读取 version" }
    return $m.Matches[0].Groups[1].Value
}

if (-not $Target) {
    $Target = Get-HostTriple
}

$Version = Get-AppVersion
Write-Host "==> 仓库: $RepoRoot"
Write-Host "==> 版本: $Version"
Write-Host "==> 目标: $Target"

if ($Target -notmatch "windows") {
    throw "当前脚本用于 Windows 目标，收到: $Target"
}

if ($Build) {
    Write-Host "==> cargo build --release --target $Target"
    cargo build --release --target $Target
    if ($LASTEXITCODE -ne 0) { throw "cargo build 失败" }
}

& (Join-Path $ScriptDir "windows/build-msi.ps1") -Target $Target
if ($LASTEXITCODE -ne 0) { throw "Windows 打包失败" }
