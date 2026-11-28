$ErrorActionPreference = 'Stop'

# The __PLACEHOLDERS__ are filled by the release workflow from the published
# release and its SHA256SUMS.txt; this file is never packed as-is.
$url64 = '__URL64__'
$checksum64 = '__CHECKSUM64__'

# Chocolatey has no ARM64 argument of its own: url64bit means "64-bit", and
# an ARM64 machine would otherwise get the x64 build under emulation. Swap
# in the native installer when the OS says it is ARM64.
if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64' -or $env:PROCESSOR_ARCHITEW6432 -eq 'ARM64') {
  $url64 = '__URLARM64__'
  $checksum64 = '__CHECKSUMARM64__'
}

$packageArgs = @{
  packageName    = $env:ChocolateyPackageName
  fileType       = 'exe'
  url64bit       = $url64
  checksum64     = $checksum64
  checksumType64 = 'sha256'
  # NSIS silent install. The installer's own hooks close a running Vitals
  # first, so an upgrade never leaves half-old files behind. The shipped setup
  # is installMode currentUser (ADR 0053), so /S is already a per-user install
  # that updates itself on quit without a UAC prompt; /CurrentUser is kept
  # because it is what keeps a `both` build per-user, where a Program Files
  # copy could not update itself (the app runs unelevated).
  silentArgs     = '/S /CurrentUser'
  validExitCodes = @(0)
  softwareName   = 'Vitals*'
}

Install-ChocolateyPackage @packageArgs
