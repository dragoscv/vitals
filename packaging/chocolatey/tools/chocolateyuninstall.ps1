$ErrorActionPreference = 'Stop'

# Finds the uninstaller the NSIS installer registered, rather than guessing
# a path: a per-machine install normally lands in Program Files, but the
# user may have chosen another directory.
[array]$keys = Get-UninstallRegistryKey -SoftwareName 'Vitals*'

if ($keys.Count -eq 0) {
  Write-Warning 'Vitals is not registered as installed; nothing to uninstall.'
  return
}
if ($keys.Count -gt 1) {
  Write-Warning "$($keys.Count) entries match 'Vitals*'; uninstalling none. Remove it from Settings > Apps instead."
  $keys | ForEach-Object { Write-Warning "- $($_.DisplayName)" }
  return
}

$uninstaller = $keys[0].UninstallString -replace '^"([^"]+)".*$', '$1'

# /S is silent. Settings in %APPDATA%\Vitals are kept on purpose, as the
# installer's own uninstall hook does: an uninstall is often a reinstall.
Uninstall-ChocolateyPackage -PackageName $env:ChocolateyPackageName `
  -FileType 'exe' `
  -SilentArgs '/S' `
  -File $uninstaller `
  -ValidExitCodes @(0)
