<#
.SYNOPSIS
  Builds and signs the sparse identity package for the unpackaged Vitals install.

.DESCRIPTION
  A SKELETON for the spike in
  docs/research/windows-widgets-sparse-package-2026-11-28.md — not a shipped
  build. It fills the {{...}} placeholders in AppxManifest.xml, packs with
  makeappx /nv (required: the referenced executables live outside the package),
  and signs with signtool.

  It FAILS CLOSED without a certificate, because a sparse package that is not
  signed by a certificate trusted on the target machine cannot be registered
  (Add-AppxPackage fails with CERT_E_UNTRUSTEDROOT, 0x800B0109). Pass -SkipSign
  only to smoke-test the manifest with makeappx; the result will NOT install.

  The certificate Subject must equal -Publisher. For local testing a self-signed
  certificate is enough, once its public .cer is in Cert:\CurrentUser\TrustedPeople.

.EXAMPLE
  # Local spike: self-signed cert whose Subject matches -Publisher.
  New-SelfSignedCertificate -Type Custom -Subject 'CN=Vitals Spike' `
    -KeyUsage DigitalSignature -FriendlyName 'Vitals spike' `
    -CertStoreLocation 'Cert:\CurrentUser\My' `
    -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3','2.5.29.19={text}')
  # ... export to .pfx, then:
  pwsh -NoProfile -File packaging/msix/sparse/build-sparse-package.ps1 `
    -IdentityName 'Vitals.Sparse' -Publisher 'CN=Vitals Spike' `
    -Version 1.6.0.0 -CertificatePath vitals-spike.pfx
#>
param(
  [Parameter(Mandatory)][string]$Publisher,
  [string]$IdentityName = 'Vitals.Sparse',
  [string]$PublisherDisplayName = 'Dragos Catalin',
  [Parameter(Mandatory)][string]$Version,
  # The CLSID the provider executable registers (CoRegisterClassObject). A real
  # one, generated once per provider: e.g. [guid]::NewGuid(). Both the
  # com:Class and the widget Activation/CreateInstance must use the same value.
  [Parameter(Mandatory)][string]$WidgetClsid,
  [string]$CertificatePath,
  [string]$CertificatePassword,
  [string]$WasdkFrameworkPackage = 'Microsoft.WindowsAppRuntime.1.8',
  [string]$WasdkMinVersion = '8000.0.0.0',
  [string]$OutDir,
  # Build the .msix without signing — it will not install. For a manifest smoke test.
  [switch]$SkipSign
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
if (-not $OutDir) { $OutDir = Join-Path $root 'target\msix-sparse' }
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

if ($Version -notmatch '^\d+\.\d+\.\d+\.\d+$') { throw "Version must be four numeric parts, got '$Version'" }
[void][guid]$WidgetClsid  # throws if not a GUID
if (-not $SkipSign -and -not $CertificatePath) {
  throw 'No -CertificatePath. A sparse package must be signed by a cert trusted on the target machine to register; pass -SkipSign only to smoke-test the manifest (the output will not install).'
}

$sdk = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin' -Directory |
  Where-Object { Test-Path (Join-Path $_.FullName 'x64\makeappx.exe') } |
  Sort-Object { [version]$_.Name } | Select-Object -Last 1
if (-not $sdk) { throw 'Windows SDK with makeappx.exe not found' }
$makeappx = Join-Path $sdk.FullName 'x64\makeappx.exe'
$signtool = Join-Path $sdk.FullName 'x64\signtool.exe'

# The layout holds only the manifest and the assets it names. The executables
# (vitals-desktop.exe, vitals-widget-provider.exe) are NOT here — they live in
# the external location (the Vitals install dir) and are bound at registration.
$layout = Join-Path $OutDir 'layout'
if (Test-Path $layout) { Remove-Item -Recurse -Force $layout }
New-Item -ItemType Directory -Force -Path (Join-Path $layout 'Assets') | Out-Null

$manifest = [IO.File]::ReadAllText((Join-Path $PSScriptRoot 'AppxManifest.xml'))
$manifest = $manifest.
  Replace('{{IDENTITY_NAME}}', $IdentityName).
  Replace('{{PUBLISHER}}', $Publisher).
  Replace('{{PUBLISHER_DISPLAY_NAME}}', $PublisherDisplayName).
  Replace('{{VERSION}}', $Version).
  Replace('{{WIDGET_CLSID}}', $WidgetClsid).
  Replace('{{WASDK_FRAMEWORK_PACKAGE}}', $WasdkFrameworkPackage).
  Replace('{{WASDK_MIN_VERSION}}', $WasdkMinVersion)
# Refuse to build with any placeholder still in the manifest.
if ($manifest -match '\{\{[A-Z0-9_]+\}\}') { throw "unfilled placeholder: $($Matches[0])" }
[IO.File]::WriteAllText((Join-Path $layout 'AppxManifest.xml'), $manifest)

$package = Join-Path $OutDir "Vitals_sparse_$($Version)_neutral.msix"
# /nv is required: the manifest references files that are not in the package.
& $makeappx pack /d $layout /p $package /o /nv
if ($LASTEXITCODE) { throw "makeappx pack failed ($LASTEXITCODE)" }
Write-Host "packed (unsigned): $package"

if ($SkipSign) {
  Write-Host 'NOT SIGNED — this package will not register. Manifest smoke test only.'
  return
}

$signArgs = @('sign', '/fd', 'SHA256', '/f', $CertificatePath)
if ($CertificatePassword) { $signArgs += @('/p', $CertificatePassword) }
& $signtool @signArgs $package
if ($LASTEXITCODE) { throw "signtool failed ($LASTEXITCODE)" }
Write-Host "signed: $package"
Write-Host "register with: packaging/msix/sparse/register-sparse-package.ps1 -PackagePath '$package' -ExternalLocation '<Vitals install dir>'"
