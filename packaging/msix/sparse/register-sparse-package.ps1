<#
.SYNOPSIS
  Registers (or unregisters) the Vitals sparse identity package for the current user.

.DESCRIPTION
  Skeleton for the spike in
  docs/research/windows-widgets-sparse-package-2026-11-28.md.

  -ExternalLocation must be the directory that actually contains
  vitals-desktop.exe and vitals-widget-provider.exe (the Vitals install dir).
  The sparse package holds only the manifest; every path in it resolves here.

  Per-user registration: it adds the package to the current user's profile and
  needs no administrator. Re-running with a newer package upgrades in place.

.EXAMPLE
  pwsh -NoProfile -File packaging/msix/sparse/register-sparse-package.ps1 `
    -PackagePath target/msix-sparse/Vitals_sparse_1.5.1.0_neutral.msix `
    -ExternalLocation "$env:LOCALAPPDATA\Programs\Vitals"
.EXAMPLE
  # Remove it again after the spike:
  pwsh -NoProfile -File packaging/msix/sparse/register-sparse-package.ps1 -Unregister
#>
param(
  [string]$PackagePath,
  [string]$ExternalLocation,
  [string]$IdentityName = 'Vitals.Sparse',
  [switch]$Unregister
)
$ErrorActionPreference = 'Stop'

if ($Unregister) {
  $pkg = Get-AppxPackage -Name $IdentityName
  if (-not $pkg) { Write-Host "not registered: $IdentityName"; return }
  Remove-AppxPackage -Package $pkg.PackageFullName
  Write-Host "unregistered: $($pkg.PackageFullName)"
  return
}

if (-not $PackagePath) { throw 'Pass -PackagePath (and -ExternalLocation) to register.' }
if (-not $ExternalLocation) { throw 'Pass -ExternalLocation = the Vitals install directory holding the executables.' }
if (-not (Test-Path $PackagePath)) { throw "package not found: $PackagePath" }
if (-not (Test-Path $ExternalLocation)) { throw "external location not found: $ExternalLocation" }

$manifest = Join-Path $ExternalLocation 'vitals-widget-provider.exe'
if (-not (Test-Path $manifest)) {
  Write-Warning "vitals-widget-provider.exe is not in $ExternalLocation. The package will register, but activating the widget will fail until the provider binary is there."
}

# Fails with CERT_E_UNTRUSTEDROOT (0x800B0109) if the signing certificate is not
# trusted on this machine — import the public .cer into
# Cert:\CurrentUser\TrustedPeople first (see the research note, §5).
try {
  Add-AppxPackage -Path $PackagePath -ExternalLocation $ExternalLocation
} catch {
  throw "Add-AppxPackage failed: $($_.Exception.Message)`nIf this is 0x800B0109, the signing certificate is not trusted on this machine."
}
$pkg = Get-AppxPackage -Name $IdentityName
Write-Host "registered: $($pkg.PackageFullName)  ->  $ExternalLocation"
Write-Host 'Now open the widgets board (Win+W) and check whether Vitals appears under "Add widgets".'
